pub mod auth;
pub mod connection;
pub mod encryption;

#[cfg(test)]
mod tests {
    use super::*;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use auth::ServerAuthenticator;
    use encryption::{AsyncDecryptor, AsyncEncryptor, Key};

    #[tokio::test]
    async fn auth_then_encrypt() {
        const PASSWORD: &[u8] = b"deadbeef";
        const SENT: &[u8] = b"Hello, world!";
        const RECEIVED: &[u8] = b"Hello, lowly ego!";

        let authenticator = ServerAuthenticator::from_password(PASSWORD).unwrap();
        let (mut client, mut server) = tokio::io::duplex(128);

        tokio::spawn(async move {
            let session_key = authenticator.login(&mut client).await.unwrap();
            let client_key = Key::from_client_session(&session_key);
            let server_key = Key::from_server_session(&session_key);
            let (rx, tx) = tokio::io::split(client);
            let mut decryptor = AsyncDecryptor::new(&client_key, rx);
            let mut encryptor = AsyncEncryptor::new(&server_key, tx);

            let mut sent = vec![0u8; SENT.len()];
            decryptor.read_exact(&mut sent).await.unwrap();
            assert_eq!(sent, SENT);

            encryptor.write_all(RECEIVED).await.unwrap();
            encryptor.flush().await.unwrap();
        });

        let session_key = auth::login(&mut server, PASSWORD).await.unwrap();
        let client_key = Key::from_client_session(&session_key);
        let server_key = Key::from_server_session(&session_key);
        let (rx, tx) = tokio::io::split(server);
        let mut decryptor = AsyncDecryptor::new(&server_key, rx);
        let mut encryptor = AsyncEncryptor::new(&client_key, tx);

        encryptor.write_all(SENT).await.unwrap();
        encryptor.flush().await.unwrap();

        let mut received = vec![0u8; RECEIVED.len()];
        decryptor.read_exact(&mut received).await.unwrap();
        assert_eq!(received, RECEIVED);
    }
}
