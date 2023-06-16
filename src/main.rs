mod ev3;
mod utils;

use ev3::project::Project;

fn main() -> anyhow::Result<()> {
    let project = Project::get_project_from_zip("examples/1block.ev3")?;
    project.output_file("out.ev3")?;
    Ok(())
}
