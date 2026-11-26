//! La sonde de santé du conteneur : `worthfold healthcheck`.
//!
//! L'image finale ne contient que le binaire (ni shell, ni curl) : la sonde est donc le binaire lui-même. Elle
//! interroge `/login` sur la machine locale, comme le ferait un navigateur, et réussit si le serveur répond
//! (la page de connexion, ou une redirection vers la configuration). Aucune donnée n'est lue, et la sonde n'a
//! besoin ni de la base ni d'une session.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::process::ExitCode;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(3);

pub fn run() -> ExitCode {
    let port = std::env::var("WORTHFOLD_PORT")
        .ok()
        .and_then(|p| p.trim().parse().ok())
        .unwrap_or(3000);
    match probe(port) {
        Ok(status) if (200..400).contains(&status) => ExitCode::SUCCESS,
        Ok(status) => {
            eprintln!("worthfold répond {status}.");
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("worthfold ne répond pas : {e}");
            ExitCode::FAILURE
        }
    }
}

/// Le code HTTP de `GET /login` sur `127.0.0.1:port`. Le `Host` est celui qu'accepte la garde des requêtes.
fn probe(port: u16) -> std::io::Result<u16> {
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let mut stream = TcpStream::connect_timeout(&address, TIMEOUT)?;
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))?;
    write!(
        stream,
        "GET /login HTTP/1.1\r\nHost: localhost:{port}\r\nConnection: close\r\n\r\n"
    )?;
    let mut head = [0u8; 32];
    let read = stream.read(&mut head)?;
    parse_status(&head[..read]).ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "réponse HTTP illisible")
    })
}

/// `HTTP/1.1 200 OK` → 200.
fn parse_status(head: &[u8]) -> Option<u16> {
    std::str::from_utf8(head)
        .ok()?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lit_le_code_de_la_ligne_d_etat() {
        assert_eq!(parse_status(b"HTTP/1.1 200 OK\r\n"), Some(200));
        assert_eq!(parse_status(b"HTTP/1.1 303 See Other\r\n"), Some(303));
        assert_eq!(parse_status(b"n'importe quoi"), None);
        assert_eq!(parse_status(b""), None);
    }
}
