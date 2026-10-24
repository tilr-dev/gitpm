//! Verified prebuilt output. Extraction never delegates to external programs.
use crate::{
    catalog,
    error::{Context, Error, Result, ensure},
    model::BuildInstall,
    transport,
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::{Read, Write},
    path::Path,
};

pub(crate) fn relative(id: &str, build: &BuildInstall) -> String {
    format!(
        "builds/{id}/{}",
        build
            .release
            .build
            .as_ref()
            .expect("build provenance")
            .sha256
    )
}

pub(crate) fn download(release: &catalog::Release, destination: &Path) -> Result<()> {
    let build = release.build.as_ref().expect("build release");
    let bytes = transport::artifact_get(&build.url)?
        .ok_or_else(|| Error::Http(format!("build artifact not found: {}", build.url)))?;
    ensure!(
        catalog::hex(&Sha256::digest(&bytes)) == build.sha256.to_ascii_lowercase(),
        Catalog,
        "build archive SHA-256 mismatch"
    );
    fs::create_dir_all(destination)?;
    let decoder = flate2::read::GzDecoder::new(bytes.as_slice());
    // Bound decompressed metadata as well as file bodies (tar extensions precede entries).
    let payload = release
        .files
        .values()
        .try_fold(0u64, |sum, file| sum.checked_add(file.size))
        .ok_or_else(|| Error::Catalog("build file sizes overflow".into()))?;
    let budget = payload
        .checked_add((release.files.len() as u64 + 1) * 1024)
        .and_then(|n| n.checked_add(16 * 1024 * 1024))
        .ok_or_else(|| Error::Catalog("build archive size overflow".into()))?;
    let mut archive = tar::Archive::new(decoder.take(budget));
    let mut seen = BTreeSet::new();
    for entry in archive
        .entries()
        .context(Error::Catalog, "invalid build archive")?
    {
        let mut entry = entry.context(Error::Catalog, "invalid build archive entry")?;
        let raw = entry.path_bytes();
        let name = std::str::from_utf8(&raw)
            .map_err(|_| Error::Catalog("invalid archive path".into()))?
            .to_owned();
        if name.trim_end_matches('/') == "." && entry.header().entry_type().is_dir() {
            continue;
        }
        let name = name
            .strip_prefix("./")
            .unwrap_or(&name)
            .trim_end_matches('/');
        let kind = entry.header().entry_type();
        ensure!(
            kind.is_file() || kind.is_dir(),
            Unsupported,
            "build archive supports only regular files and directories"
        );
        if name == "." && kind.is_dir() {
            continue;
        }
        crate::packages::validate_path(name)
            .map_err(|_| Error::Catalog("unsafe build archive path".into()))?;
        ensure!(
            !name.contains('\\') && seen.insert(name.to_owned()),
            Catalog,
            "duplicate or unsafe build archive path: {name}"
        );
        let target = destination.join(name);
        if kind.is_dir() {
            ensure!(entry.size() == 0, Catalog, "directory entry contains data");
            fs::create_dir_all(&target)?;
            continue;
        }
        let expected = release
            .files
            .get(name)
            .ok_or_else(|| Error::Catalog(format!("unexpected build file: {name}")))?;
        ensure!(
            entry.size() == expected.size,
            Catalog,
            "build file size mismatch: {name}"
        );
        fs::create_dir_all(target.parent().unwrap())?;
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)?;
        let copied =
            std::io::copy(&mut entry, &mut output).context(Error::Catalog, "invalid build file")?;
        output.flush()?;
        ensure!(
            copied == expected.size,
            Catalog,
            "truncated build file: {name}"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = entry
                .header()
                .mode()
                .context(Error::Catalog, "invalid build file mode")?;
            fs::set_permissions(
                &target,
                fs::Permissions::from_mode(if mode & 0o111 != 0 { 0o755 } else { 0o644 }),
            )?;
        }
    }
    // Consume the gzip trailer too: a corrupt CRC or truncated trailer is not valid output.
    let mut decoder = archive.into_inner();
    let mut padding = [0u8; 8192];
    let mut total = 0;
    loop {
        let n = decoder
            .read(&mut padding)
            .context(Error::Catalog, "invalid gzip trailer")?;
        if n == 0 {
            break;
        }
        total += n;
        ensure!(
            total <= 16 * 1024 * 1024 && padding[..n].iter().all(|b| *b == 0),
            Catalog,
            "trailing build archive data"
        );
    }
    ensure!(
        decoder.limit() != 0,
        Catalog,
        "build archive exceeds declared output size"
    );
    ensure!(
        catalog::hash_files(destination)? == release.files,
        Catalog,
        "catalog file hashes do not match build output"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Manifest, catalog::FileDigest, test_support::TempDir};
    use std::{collections::BTreeMap, net::TcpListener, thread};

    #[test]
    fn traversal_is_refused_even_when_listed_in_release_files() {
        let temp = TempDir::new("artifact-traversal");
        let destination = temp.join("staging");
        let name = "../escape";
        let contents = b"escaped";
        let mut header = tar::Header::new_gnu();
        header.as_mut_bytes()[..name.len()].copy_from_slice(name.as_bytes());
        header.set_size(contents.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let mut archive = tar::Builder::new(encoder);
        archive.append(&header, contents.as_slice()).unwrap();
        let bytes = archive.into_inner().unwrap().finish().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/build.tar.gz", listener.local_addr().unwrap());
        let hash = catalog::hex(&Sha256::digest(&bytes));
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut byte = [0];
            while !request.ends_with(b"\r\n\r\n") {
                assert_eq!(stream.read(&mut byte).unwrap(), 1);
                request.push(byte[0]);
            }
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                bytes.len()
            )
            .unwrap();
            stream.write_all(&bytes).unwrap();
        });
        // Catalog validation normally refuses unsafe files keys first. Call the
        // extractor directly to prove its guard works independently of that layer.
        let release = catalog::Release {
            manifest: Manifest::default(),
            source: String::new(),
            commit: String::new(),
            reference: String::new(),
            files: BTreeMap::from([(
                name.to_owned(),
                FileDigest {
                    sha256: catalog::hex(&Sha256::digest(contents)),
                    size: contents.len() as u64,
                },
            )]),
            build: Some(catalog::Build { url, sha256: hash }),
            path: None,
        };
        let result = download(&release, &destination);
        server.join().unwrap();
        assert!(!temp.join("escape").exists(), "wrote outside staging");
        let error = result.unwrap_err();
        assert!(
            error.to_string().contains("unsafe build archive path"),
            "{error}"
        );
    }
}
