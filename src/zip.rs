use std::{
    fs::{self, File},
    io::{Read, Seek, Write},
    path::Path,
};

use anyhow::anyhow;

use crate::Result;

pub fn pack<P: AsRef<Path>>(srcpath: P) -> Result<Vec<u8>> {
    let file = tempfile::NamedTempFile::new()?;
    let srcpath = srcpath.as_ref();
    let dstpath = file.path();
    let mut content = vec![];
    if srcpath.is_dir() {
        let file = File::create(dstpath)?;
        let walkdir = walkdir::WalkDir::new(srcpath);
        let it = walkdir.into_iter();
        zip_dir(
            &mut it.filter_map(|e| e.ok()),
            srcpath,
            file,
            zip::CompressionMethod::Zstd,
        )?;
        let mut f = File::open(dstpath)?;
        f.read_to_end(&mut content)?;
        return Ok(content);
    }
    if srcpath.is_file() {
        return Ok(content);
    }
    Err(anyhow!(
        "{} is neither a file or directory",
        srcpath.display()
    ))?
}

pub fn unpack<B: AsRef<[u8]>, P: AsRef<Path>>(buf: B, dstdir: P) -> Result<()> {
    let buf = std::io::Cursor::new(buf.as_ref());
    let dstdir = dstdir.as_ref();
    if dstdir.exists() {
        if !dstdir.is_dir() {
            Err(anyhow!(
                "zip unpack destination {} must be a directory",
                dstdir.display()
            ))?;
        }
    } else {
        fs::create_dir_all(dstdir)?;
    }
    let mut archiver = zip::ZipArchive::new(buf)?;
    archiver.extract(dstdir)?;
    Ok(())
}

fn zip_dir<T, P>(
    it: &mut dyn Iterator<Item = walkdir::DirEntry>,
    prefix: P,
    writer: T,
    method: zip::CompressionMethod,
) -> Result<()>
where
    T: Write + Seek,
    P: AsRef<Path>,
{
    let mut zip = zip::ZipWriter::new(writer);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(method)
        .unix_permissions(0o755);
    let prefix = prefix.as_ref();

    let mut buffer = Vec::new();
    for entry in it {
        let path = entry.path();
        let name = path
            .strip_prefix(prefix)?
            .to_str()
            .ok_or(anyhow!("strip prefix {} failed", prefix.display()))?;
        if path.is_file() {
            zip.start_file(name, options)?;
            let mut f = File::open(path)?;

            f.read_to_end(&mut buffer)?;
            zip.write_all(&buffer)?;
            buffer.clear();
        } else if !name.is_empty() {
            zip.add_directory(name, options)?;
        }
    }
    zip.finish()?;
    Ok(())
}

