use anyhow::Context;
use quick_xml::{
    events::{BytesDecl, Event},
    reader::Reader,
};
use std::fs::File;
use std::io::Read;
use std::str;

struct EV3Project {
    title: String,
    description: String,
    year: usize,
    /// Amazing, I can change the thumbnail to my project and insert naked men
    thumbnail: Vec<u8>,
    /// I don't know what this is, something or other
    activity: String,
    /// ????
    activity_assets: Vec<u8>,
    /// I assume there's no need to parse this, we don't change it
    project: String,

    files: Vec<EV3File>,
}

struct EV3File {
    decl: BytesDecl<'static>,
    name: String,
    contents: Vec<u8>,
}

impl EV3File {
    fn new(name: &str, contents: Vec<u8>) -> anyhow::Result<Self> {
        let mut xml = Reader::from_reader(contents.as_slice());
        xml.trim_text(true);
        let mut decl = None;
        loop {
            match xml.read_event().context("File had invalid xml")? {
                Event::Start(t) => {
                    println!("Start tag: name {}", str::from_utf8(t.name().0).unwrap())
                }
                Event::End(t) => println!("End tag: name {}", str::from_utf8(t.name().0).unwrap()),
                Event::Empty(t) => {
                    println!("Empty tag: name {}", str::from_utf8(t.name().0).unwrap())
                }
                Event::Text(_) => println!("Found Text"),
                Event::Comment(_) => println!("Ignoring Comment"),
                Event::CData(_) => println!("Found CData"),
                Event::Decl(d) => decl = Some(d),
                Event::PI(_) => println!("Found Processing"),
                Event::DocType(_) => println!("Found DocType"),
                Event::Eof => break,
            }
        }
        let decl = decl.context("Should have a decl")?;
        todo!()
    }
}

fn get_project_from_zip(filename: &str) -> anyhow::Result<EV3Project> {
    let file = File::open(filename)?;
    let mut zip = zip::ZipArchive::new(file).context("Failed to read zip file")?;

    let mut title = None;
    let mut description = None;
    let mut year = None;
    let mut thumbnail = None;
    let mut activity_assets = None;
    let mut activity = None;
    let mut project = None;
    let mut files = vec![];

    for i in 0..zip.len() {
        let mut z = zip.by_index(i).context("Zip library doesn't work lol")?;

        let name = z
            .enclosed_name()
            .context("Name was invalid")?
            .to_str()
            .unwrap()
            .to_owned();

        let mut bytes = vec![];
        z.read_to_end(&mut bytes)?;

        match name.as_str() {
            "___CopyrightYear" => {
                year = Some(
                    bytes
                        .iter()
                        .fold(0, |acc, &digit| acc * 10 + (digit - 48) as usize),
                )
            }
            "___ProjectDescription" => {
                description = Some(String::from_utf8(bytes).context("Invalid description data")?)
            }
            "___ProjectTitle" => {
                title = Some(String::from_utf8(bytes).context("Invalid project title")?)
            }
            "___ProjectThumbnail" => thumbnail = Some(bytes),
            "ActivityAssets.laz" => activity_assets = Some(bytes),
            "Activity.x3a" => {
                activity = Some(String::from_utf8(bytes).context("Invalid activity(?) data")?)
            }
            "Project.lvprojx" => {
                project = Some(String::from_utf8(bytes).context("Invalid project file")?)
            }

            _ => {
                let name = name.as_str();
                files.push(EV3File::new(name, bytes).context("Failed parsing")?);
            }
        }
    }
    let title = title.context("Found no title")?;
    let description = description.context("Found no description")?;
    let year = year.context("Found no year")?;
    let thumbnail = thumbnail.context("Found no thumbnail")?;
    let activity = activity.context("Found no activity")?;
    let activity_assets = activity_assets.context("Found no activity_assets")?;
    let project = project.context("Found no project")?;
    println!("Found title `{}`", title);
    println!("Found description `{}`", description);
    println!("Found year {}", year);

    Ok(EV3Project {
        title,
        description,
        year,
        thumbnail,
        activity,
        activity_assets,
        files,
        project,
    })
}

fn main() -> anyhow::Result<()> {
    get_project_from_zip("test.ev3")?;
    Ok(())
}
