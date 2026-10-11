//! Native "build" support: turn a Z program into a standalone executable.
//!
//! The Z toolchain is a tree-walking interpreter, so "compiling to binary"
//! bundles the program together with the interpreter that already ships the
//! lexer, parser, analyzer, VM and standard library. [`build_executable`]
//! copies the running `z` executable and appends the Z source behind a small,
//! self-describing trailer. The result is a real native ELF/PE/Mach-O binary
//! that runs the program with no `.z` source file and no separate `z` install.
//!
//! When a built binary starts, [`embedded_source`] detects the appended
//! payload and the CLI runs it directly instead of parsing command-line
//! arguments — so the artifact behaves like any other compiled program.

use crate::error::{ZError, ZResult};
use std::path::Path;

/// Trailer written after every embedded payload. The magic lets a built binary
/// recognise that it carries a program, and the version byte guards against
/// reading a payload produced by an incompatible toolchain.
const MAGIC: &[u8; 8] = b"ZBUILD\x00\x01";

/// Number of trailing bytes framing the payload: an 8-byte little-endian length
/// followed by the 8-byte magic.
const TRAILER_LEN: usize = 16;

/// Build a standalone executable at `output` that runs `source`.
///
/// The artifact is a byte-for-byte copy of the current interpreter with `source`
/// appended behind the framing trailer, so it reuses the entire tested runtime.
pub fn build_executable(source: &str, output: &Path) -> ZResult<()> {
    let exe = std::env::current_exe().map_err(|e| {
        ZError::Runtime(format!("cannot locate the z interpreter: {}", e))
    })?;
    let base = std::fs::read(&exe).map_err(|e| {
        ZError::Runtime(format!("cannot read interpreter '{}': {}", exe.display(), e))
    })?;

    let bundle = make_bundle(&base, source);

    std::fs::write(output, &bundle).map_err(|e| {
        ZError::Runtime(format!("cannot write '{}': {}", output.display(), e))
    })?;
    mark_executable(output)?;
    Ok(())
}

/// Assemble the bytes of a built executable: `base` interpreter image followed
/// by `source` and the framing trailer. Split out so it can be unit-tested
/// without touching the filesystem.
fn make_bundle(base: &[u8], source: &str) -> Vec<u8> {
    let payload = source.as_bytes();
    let mut out = Vec::with_capacity(base.len() + payload.len() + TRAILER_LEN);
    out.extend_from_slice(base);
    out.extend_from_slice(payload);
    out.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    out.extend_from_slice(MAGIC);
    out
}

/// If the running executable carries an embedded Z program, return its source.
///
/// Reads only the trailer and payload rather than the whole image, so the
/// per-startup cost stays small even for a multi-megabyte interpreter.
pub fn embedded_source() -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};

    let exe = std::env::current_exe().ok()?;
    let mut file = std::fs::File::open(&exe).ok()?;
    let total = file.metadata().ok()?.len();
    if total < TRAILER_LEN as u64 {
        return None;
    }

    file.seek(SeekFrom::End(-(TRAILER_LEN as i64))).ok()?;
    let mut trailer = [0u8; TRAILER_LEN];
    file.read_exact(&mut trailer).ok()?;
    let (len_bytes, magic) = trailer.split_at(8);
    if magic != MAGIC.as_slice() {
        return None;
    }

    let len = u64::from_le_bytes(len_bytes.try_into().ok()?) as usize;
    if len == 0 {
        return None;
    }
    let payload_start = total.checked_sub(TRAILER_LEN as u64)?.checked_sub(len as u64)?;
    file.seek(SeekFrom::Start(payload_start)).ok()?;
    let mut buf = vec![0u8; len];
    file.read_exact(&mut buf).ok()?;
    String::from_utf8(buf).ok()
}

/// Extract an embedded payload from a full executable image. `None` when the
/// bytes are a plain interpreter with no trailing program.
///
/// Used by the unit tests and as the reference for the trailer layout; the
/// production path ([`embedded_source`]) streams just the trailer instead of
/// loading the whole image.
#[cfg(test)]
fn read_payload(data: &[u8]) -> Option<String> {
    if data.len() < TRAILER_LEN {
        return None;
    }
    let (rest, magic) = data.split_at(data.len() - 8);
    if magic != MAGIC.as_slice() {
        return None;
    }
    let (body, len_bytes) = rest.split_at(rest.len() - 8);
    let len = u64::from_le_bytes(len_bytes.try_into().ok()?) as usize;
    if len == 0 || len > body.len() {
        return None;
    }
    let start = body.len() - len;
    String::from_utf8(body[start..].to_vec()).ok()
}

#[cfg(unix)]
fn mark_executable(path: &Path) -> ZResult<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)
        .map_err(|e| ZError::Runtime(format!("cannot stat '{}': {}", path.display(), e)))?
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms)
        .map_err(|e| ZError::Runtime(format!("cannot mark '{}' executable: {}", path.display(), e)))
}

#[cfg(not(unix))]
fn mark_executable(_path: &Path) -> ZResult<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "fn Main() {\n    call printer(\"built\")\n}\n";

    #[test]
    fn roundtrip_extracts_source() {
        let base = b"FAKE-INTERPRETER-IMAGE";
        let bundle = make_bundle(base, SAMPLE);
        assert_eq!(read_payload(&bundle).as_deref(), Some(SAMPLE));
        // The interpreter image is preserved verbatim at the front.
        assert!(bundle.starts_with(base));
    }

    #[test]
    fn plain_interpreter_has_no_payload() {
        assert!(read_payload(b"an ordinary binary with no trailer").is_none());
        assert!(read_payload(b"").is_none());
    }

    #[test]
    fn wrong_magic_is_rejected() {
        let mut data = make_bundle(b"base", SAMPLE);
        let n = data.len();
        data[n - 1] ^= 0xFF; // corrupt the magic
        assert!(read_payload(&data).is_none());
    }

    #[test]
    fn oversized_length_is_rejected() {
        let mut data = make_bundle(b"base", SAMPLE);
        let n = data.len();
        // Drop the real magic, then claim a length larger than the body.
        data.truncate(n - 8);
        data.extend_from_slice(&(1_000_000u64).to_le_bytes());
        data.extend_from_slice(MAGIC);
        assert!(read_payload(&data).is_none());
    }

    #[test]
    fn empty_source_is_rejected() {
        let bundle = make_bundle(b"base", "");
        assert!(read_payload(&bundle).is_none());
    }
}

