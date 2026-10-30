use super::*;
pub(super) async fn prepare(root: &Path, store: &postgres::PgStore) -> Result<Simulation> {
    let roster = root.join("company.json");
    if roster.exists() {
        let value: Value = serde_json::from_slice(&std::fs::read(roster)?)?;
        return Ok(Simulation {
            employees: serde_json::from_value(value["employees"].clone())?,
            workspace: value["workspace"]
                .as_str()
                .context("Invalid roster")?
                .into(),
            root: root.into(),
            paused: std::sync::atomic::AtomicBool::new(false),
        });
    }
    let names = [
        "Ananya Rao",
        "Vikram Shah",
        "Meera Nair",
        "Arjun Patel",
        "Kavya Reddy",
        "Rohan Das",
        "Priya Menon",
        "Dev Singh",
        "Isha Jain",
        "Nikhil Kumar",
        "Sneha Roy",
        "Aditya Bose",
        "Tara Kapoor",
        "Rahul Verma",
        "Neha Joshi",
        "Asha Iyer",
        "Kabir Ali",
        "Pooja Sen",
        "Sahil Gupta",
        "Diya Thomas",
    ];
    let mut employees = Vec::new();
    for (number, name) in names.iter().enumerate() {
        let user = store
            .login(
                &format!("simulation_employee_{number}"),
                &format!("employee{number}@company.example.test"),
                &digest(&random_token()),
                "local-simulation",
            )
            .await?;
        let user = store.update_profile(&user, name).await?;
        let team = match number {
            0..=2 => "Management",
            3..=6 => "Sales",
            7..=11 => "Operations",
            12..=14 => "Finance",
            _ => "Support",
        };
        employees.push(Employee {
            number,
            name: (*name).into(),
            team: team.into(),
            user,
        });
    }
    let owner = &employees[0].user;
    let workspace = store
        .create(
            owner,
            "Aster Distribution · Simulation",
            "a57e0000-0000-4000-8000-000000000001",
        )
        .await?;
    for employee in employees.iter().skip(1) {
        if store.role(&workspace, &employee.user.id).await.is_err() {
            let id = store
                .reserve_invite(owner, &workspace, &employee.user.email, "Member")
                .await?;
            store
                .delivered(&id, Some("simulation-local-no-email"))
                .await?;
            store.accept(&employee.user, &id).await?;
        }
    }
    let simulation = Simulation {
        employees,
        workspace,
        root: root.into(),
        paused: std::sync::atomic::AtomicBool::new(false),
    };
    let bytes = serde_json::to_vec_pretty(
        &json!({"workspace":simulation.workspace,"employees":simulation.employees}),
    )?;
    std::fs::write(root.join("company.json.new"), bytes)?;
    std::fs::rename(root.join("company.json.new"), roster)?;
    Ok(simulation)
}
