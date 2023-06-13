mod ev3;
mod utils;

use ev3::project::EV3Project;

fn main() -> anyhow::Result<()> {
    let project = EV3Project::get_project_from_zip("1block.ev3")?;
    project.output_file("out.ev3")?;
    Ok(())
}
