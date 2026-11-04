//! Symbol resolution in the tracee's loaded ELF images.

use std::collections::HashMap;

use object::{Object, ObjectSegment, ObjectSymbol};

/// A mapped ELF image from `/proc/<pid>/maps`.
#[derive(Debug, Clone)]
pub struct Image {
    /// File path.
    pub path: String,
    /// Lowest mapped address of the image.
    pub base: u64,
}

/// One region line from `/proc/<pid>/maps`.
#[derive(Debug, Clone)]
pub struct MapLine {
    /// Start address.
    pub start: u64,
    /// End address.
    pub end: u64,
    /// Permissions, for example `r-xp`.
    pub perms: String,
    /// File offset.
    pub offset: u64,
    /// Path or pseudo-path.
    pub path: Option<String>,
}

/// Parse `/proc/<pid>/maps` text.
#[must_use]
pub fn parse_maps(text: &str) -> Vec<MapLine> {
    text.lines()
        .filter_map(|l| {
            let mut f = l.split_whitespace();
            let range = f.next()?;
            let perms = f.next()?.to_owned();
            let offset = u64::from_str_radix(f.next()?, 16).ok()?;
            let _dev = f.next()?;
            let _inode = f.next()?;
            let path = f.next().map(str::to_owned);
            let (s, e) = range.split_once('-')?;
            Some(MapLine {
                start: u64::from_str_radix(s, 16).ok()?,
                end: u64::from_str_radix(e, 16).ok()?,
                perms,
                offset,
                path,
            })
        })
        .collect()
}

/// File-backed ELF images and their load bases.
#[must_use]
pub fn images(maps: &[MapLine]) -> Vec<Image> {
    let mut by_path: HashMap<&str, u64> = HashMap::new();
    for m in maps {
        if let Some(p) = &m.path
            && p.starts_with('/')
        {
            let base = m.start - m.offset;
            by_path
                .entry(p)
                .and_modify(|b| *b = (*b).min(base))
                .or_insert(base);
        }
    }
    let mut v: Vec<Image> = by_path
        .into_iter()
        .map(|(path, base)| Image {
            path: path.to_owned(),
            base,
        })
        .collect();
    v.sort_by_key(|i| i.base);
    v
}

/// Resolve `names` in an image to runtime addresses. Looks at `.dynsym` and
/// `.symtab`; only defined function symbols count.
///
/// # Errors
/// Fails if the file cannot be read or parsed.
pub fn resolve(image: &Image, names: &[&str]) -> Result<HashMap<String, u64>, String> {
    let data = std::fs::read(&image.path).map_err(|e| format!("{}: {e}", image.path))?;
    let file = object::File::parse(&*data).map_err(|e| format!("{}: {e}", image.path))?;
    // Runtime address = base + (symbol vaddr - lowest segment vaddr).
    let first_vaddr = file.segments().map(|s| s.address()).min().unwrap_or(0);
    let mut out = HashMap::new();
    for sym in file.dynamic_symbols().chain(file.symbols()) {
        if !sym.is_definition() || sym.kind() != object::SymbolKind::Text {
            continue;
        }
        let Ok(name) = sym.name() else { continue };
        if names.contains(&name) && !out.contains_key(name) {
            out.insert(name.to_owned(), image.base + (sym.address() - first_vaddr));
        }
    }
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unreadable_literal)]
mod tests {
    use super::*;

    #[test]
    fn parses_maps_lines() {
        let text = "00400000-00401000 r--p 00000000 08:02 123 /usr/bin/x\n\
                    00401000-00402000 r-xp 00001000 08:02 123 /usr/bin/x\n\
                    7ffff7d00000-7ffff7d28000 r--p 00000000 08:02 9 /usr/lib/libc.so.6\n\
                    7ffffffde000-7ffffffff000 rw-p 00000000 00:00 0 [stack]\n";
        let m = parse_maps(text);
        assert_eq!(m.len(), 4);
        assert_eq!(m[3].path.as_deref(), Some("[stack]"));
        let imgs = images(&m);
        assert_eq!(imgs.len(), 2);
        assert_eq!(imgs[0].base, 0x400000);
    }
}
