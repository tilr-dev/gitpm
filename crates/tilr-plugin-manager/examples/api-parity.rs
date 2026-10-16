//! Small public-API driver for the Dagger CLI/library parity scenario.
use tilr_plugin_manager::Manager;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let manager = Manager::open(&args[1])?;
    let id = &args[3];
    match args[2].as_str() {
        "install" => {
            manager.install_ref(id, args.get(4).map(String::as_str), None)?;
        }
        "update" => {
            manager.update_ref(id, args.get(4).map(String::as_str))?;
        }
        "rollback" => {
            manager.rollback(id)?;
        }
        "disable" => manager.disable(id)?,
        "enable" => manager.enable(id)?,
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
