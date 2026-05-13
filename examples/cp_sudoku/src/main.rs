//! Demonstrates CpSatSuggestor with a 4x4 mini-Sudoku.
//!
//! Grid positions [r][c] each need values 1-4.
//! Rows, columns, and 2x2 boxes are all-different.

use converge_core::{ContextState, Engine};
use converge_pack::ContextKey;
use ferrox::cp::{ConstraintKind, CpSatPlan, CpSatRequest, CpSatSuggestor, CpVariable};

#[tokio::main]
async fn main() {
    let mut engine = Engine::new();
    engine.register_suggestor(CpSatSuggestor);

    // Build 4x4 Sudoku: variables x_rc, domain [1,4]
    let mut variables = Vec::new();
    for r in 0..4 {
        for c in 0..4 {
            variables.push(CpVariable {
                name: format!("x_{r}{c}"),
                lb: 1,
                ub: 4,
                is_bool: false,
            });
        }
    }

    let cell = |r: usize, c: usize| format!("x_{r}{c}");

    let mut constraints = Vec::new();

    // Row all-different
    for r in 0..4 {
        let vars: Vec<_> = (0..4).map(|c| cell(r, c)).collect();
        constraints.push(ConstraintKind::AllDifferent { vars });
    }

    // Column all-different
    for c in 0..4 {
        let vars: Vec<_> = (0..4).map(|r| cell(r, c)).collect();
        constraints.push(ConstraintKind::AllDifferent { vars });
    }

    // 2x2 boxes all-different
    for br in [0, 2] {
        for bc in [0, 2] {
            let vars = vec![
                cell(br, bc),
                cell(br, bc + 1),
                cell(br + 1, bc),
                cell(br + 1, bc + 1),
            ];
            constraints.push(ConstraintKind::AllDifferent { vars });
        }
    }

    // Seed: fix x_00 = 1 (i.e., x_00 == 1)
    constraints.push(ConstraintKind::LinearEq {
        terms: vec![ferrox::cp::CpTerm {
            var: cell(0, 0),
            coeff: 1,
        }],
        rhs: 1,
    });

    let request = CpSatRequest {
        id: "sudoku-4x4".to_string(),
        variables,
        interval_vars: vec![],
        optional_interval_vars: vec![],
        constraints,
        objective_terms: None,
        minimize: false,
        time_limit_seconds: Some(10.0),
    };

    let mut ctx = ContextState::new();
    ctx.add_input(
        ContextKey::Seeds,
        "cpsat-request:sudoku-4x4",
        &serde_json::to_string(&request).unwrap(),
    )
    .unwrap();

    let result = engine.run(ctx).await.unwrap();

    let plans: Vec<_> = result
        .context
        .get(ContextKey::Strategies)
        .iter()
        .filter_map(|f| serde_json::from_str::<CpSatPlan>(f.content()).ok())
        .collect();

    for plan in &plans {
        println!("Status: {}", plan.status);
        println!("Solver: {}", plan.solver);
        println!("Time:   {:.3}s", plan.wall_time_seconds);
        println!();

        let mut grid = [[0i64; 4]; 4];
        for (name, val) in &plan.assignments {
            let r = name.chars().nth(2).unwrap().to_digit(10).unwrap() as usize;
            let c = name.chars().nth(3).unwrap().to_digit(10).unwrap() as usize;
            grid[r][c] = *val;
        }
        for row in &grid {
            println!("{:?}", row);
        }
    }
}
