//! Oracle correctness properties on every workload.

use morphstack_cost::{CostModel, Migration, Rep};
use morphstack_sim::{Baseline, Workload, oracle, price, price_context};
use morphstack_workloads::{Spec, Workload as Gen, generate_vec, header};

fn load(w: Gen, seed: u64) -> Workload {
    let spec = Spec {
        workload: w,
        seed,
        scale: 0.02,
    };
    let events = generate_vec(&spec);
    Workload::from_events::<()>(&header(&spec), events.into_iter().map(Ok)).unwrap()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0)
}

#[test]
fn oracle_cost_matches_repricing_its_schedule() {
    let m = CostModel::default();
    for g in Gen::ALL {
        let w = load(g, 3);
        for c in &w.contexts {
            let (reps, dp) = oracle::solve_context(&m, c);
            let priced = price_context(&m, c, &reps, None).cost.total();
            assert!(
                close(dp, priced),
                "{}: dp {dp} vs priced {priced}",
                g.name()
            );
        }
    }
}

#[test]
fn oracle_never_loses_to_any_baseline() {
    let m = CostModel::default();
    for g in Gen::ALL {
        let w = load(g, 4);
        let best = price(&m, &w, &oracle::solve(&m, &w)).cost.total();
        for b in Baseline::ALL {
            let c = price(&m, &w, &b.schedule(&w)).cost.total();
            assert!(
                best <= c * (1.0 + 1e-12),
                "{}: oracle {best} > {} {c}",
                g.name(),
                b.name()
            );
        }
    }
}

#[test]
fn infinite_migration_cost_reduces_oracle_to_best_fixed_per_context() {
    let mut m = CostModel::default();
    let inf = Migration {
        fixed_ns: 1e30,
        ns_per_byte: 0.0,
        ns_per_frame: 0.0,
    };
    m.migration = [[inf; 3]; 3];
    for g in Gen::ALL {
        let w = load(g, 5);
        for c in &w.contexts {
            let (reps, dp) = oracle::solve_context(&m, c);
            assert!(
                reps.windows(2).all(|p| p[0] == p[1]),
                "{} migrated",
                g.name()
            );
            let best_fixed = Rep::ALL
                .iter()
                .map(|&r| {
                    price_context(&m, c, &vec![r; c.steps.len()], None)
                        .cost
                        .total()
                })
                .fold(f64::INFINITY, f64::min);
            assert!(close(dp, best_fixed), "{}: {dp} vs {best_fixed}", g.name());
        }
    }
}

#[test]
fn oracle_is_bounded_below_by_the_per_step_minimum() {
    let m = CostModel::default();
    let w = load(Gen::W6PhaseChange, 6);
    for c in w.contexts.iter().take(200) {
        let (_, dp) = oracle::solve_context(&m, c);
        let per_step: f64 = c
            .steps
            .iter()
            .map(|s| {
                Rep::ALL
                    .iter()
                    .map(|&r| m.step(r, s).total())
                    .fold(f64::INFINITY, f64::min)
            })
            .sum();
        assert!(dp >= per_step * (1.0 - 1e-12), "{dp} < {per_step}");
    }
}

#[test]
fn hindsight_static_sits_between_best_fixed_and_oracle() {
    let m = CostModel::default();
    for g in Gen::ALL {
        let w = load(g, 8);
        let oracle_cost = price(&m, &w, &oracle::solve(&m, &w)).cost.total();
        let stat = price(&m, &w, &morphstack_sim::hindsight_static(&m, &w))
            .cost
            .total();
        let best_fixed = Rep::ALL
            .iter()
            .map(|&r| price(&m, &w, &Baseline::Fixed(r).schedule(&w)).cost.total())
            .fold(f64::INFINITY, f64::min);
        assert!(oracle_cost <= stat * (1.0 + 1e-12), "{}", g.name());
        assert!(stat <= best_fixed * (1.0 + 1e-12), "{}", g.name());
    }
}

#[test]
fn baselines_are_priced_consistently() {
    let m = CostModel::default();
    let w = load(Gen::Server, 7);
    for b in Baseline::ALL {
        let o = price(&m, &w, &b.schedule(&w));
        assert_eq!(o.migrations, 0, "{}", b.name());
        assert!(o.peak_bytes > 0 && o.mean_bytes > 0.0);
        assert!(close(o.time_share.iter().sum(), 1.0));
    }
}

#[test]
fn ideal_bound_is_below_the_oracle() {
    let m = CostModel::default();
    for g in Gen::ALL {
        let w = load(g, 9);
        let bound = morphstack_sim::ideal_bound(&m, &w).total();
        let best = price(&m, &w, &oracle::solve(&m, &w)).cost.total();
        assert!(
            bound <= best * (1.0 + 1e-12),
            "{}: bound {bound} > oracle {best}",
            g.name()
        );
    }
}
