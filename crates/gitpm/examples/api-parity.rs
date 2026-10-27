//! Small public-API driver for the Dagger CLI/library parity scenario.
use gitpm::Manager;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let manager = Manager::open(&args[1])?;
    let id = &args[3];
    match args[2].as_str() {
        "install" => {
            let target = match args.get(4) {
                Some(reference) => gitpm::InstallTarget::Ref(reference.clone()),
                None => gitpm::InstallTarget::Release,
            };
            manager.install_package(id, None, target)?;
        }
        "update" => {
            let target = match args.get(4) {
                Some(reference) => gitpm::Target::Ref(reference.clone()),
                None => gitpm::Target::Recorded,
            };
            manager.update_to(id, target)?;
        }
        "rollback" => {
            manager.rollback(id)?;
        }
        "disable" => {
            manager.disable(id)?;
        }
        "enable" => {
            manager.enable(id)?;
        }
        "forget" => {
            manager.forget(id)?;
        }
        "restore" => {
            manager.restore(id)?;
        }
        _ => return Err("unsupported parity operation".into()),
    }
    println!("{}", serde_json::to_string(&manager.state()?)?);
    Ok(())
}
