//! Demonstrates HighsMipSuggestor with a capital budgeting problem.
//!
//! Budget: $10M. Choose projects to maximize NPV.
//! Each project is a binary variable (fund or not).

use converge_core::{ContextState, Engine};
use converge_pack::{ContextKey, ProposedFact};
use ferrox::mip::{
    HighsMipSuggestor, MipConstraint, MipObjective, MipPlan, MipRequest, MipTerm, MipVariable,
    VarKind,
};

#[tokio::main]
async fn main() {
    let mut engine = Engine::new();
    engine.register_suggestor(HighsMipSuggestor);

    // Projects: (name, NPV $M, Cost $M)
    let projects = [
        ("alpha", 8.0, 4.0),
        ("beta", 5.0, 2.0),
        ("gamma", 6.0, 3.0),
        ("delta", 3.0, 1.5),
        ("epsilon", 7.0, 3.5),
    ];

    let variables: Vec<_> = projects
        .iter()
        .map(|(name, _, _)| MipVariable {
            name: name.to_string(),
            lb: 0.0,
            ub: 1.0,
            kind: VarKind::Binary,
        })
        .collect();

    // Budget constraint: sum(cost_i * x_i) <= 10
    let budget_terms: Vec<_> = projects
        .iter()
        .map(|(name, _, cost)| MipTerm {
            var: name.to_string(),
            coeff: *cost,
        })
        .collect();

    let constraints = vec![MipConstraint {
        name: "budget".to_string(),
        lb: f64::NEG_INFINITY,
        ub: 10.0,
        terms: budget_terms,
    }];

    // Maximize total NPV
    let obj_terms: Vec<_> = projects
        .iter()
        .map(|(name, npv, _)| MipTerm {
            var: name.to_string(),
            coeff: *npv,
        })
        .collect();

    let request = MipRequest {
        id: "capbudget".to_string(),
        variables,
        constraints,
        objective: MipObjective {
            terms: obj_terms,
            maximize: true,
        },
        time_limit_seconds: Some(30.0),
        mip_gap_tolerance: Some(1e-4),
    };

    let mut ctx = ContextState::new();
    ctx.add_proposal(ProposedFact::new(
        ContextKey::Seeds,
        "mip-request:capbudget",
        request,
        "highs-mip-example",
    ))
    .unwrap();

    let result = engine.run(ctx).await.unwrap();

    for fact in result.context.get(ContextKey::Strategies) {
        if let Some(plan) = fact.payload::<MipPlan>() {
            println!("Status:     {}", plan.status);
            println!("Solver:     {}", plan.solver);
            println!("NPV:        ${:.1}M", plan.objective_value);
            println!("MIP gap:    {:.4}", plan.mip_gap);
            println!();
            println!("Projects funded:");
            for (name, val) in &plan.values {
                if *val > 0.5 {
                    let cost = projects
                        .iter()
                        .find(|(n, _, _)| *n == name)
                        .map(|(_, _, c)| c)
                        .unwrap();
                    println!("  {name:10}  cost=${cost:.1}M");
                }
            }
        }
    }
}
