use chacha20poly1305::aead::OsRng;
use opaque_ke::{CipherSuite, ClientRegistration, ServerRegistration, ServerSetup};
use tokio::io::{AsyncRead, AsyncWrite};

#[derive(Debug)]
pub enum AuthenticationError {
    IoError(std::io::Error),
}

impl From<std::io::Error> for AuthenticationError {
    fn from(err: std::io::Error) -> Self {
        AuthenticationError::IoError(err)
    }
}

struct CS;

impl CipherSuite for CS {
    type OprfCs = opaque_ke::Ristretto255;
    type KeGroup = opaque_ke::Ristretto255;
    type KeyExchange = opaque_ke::key_exchange::tripledh::TripleDh;
    type Ksf = argon2::Argon2<'static>;
}

pub struct ServerListener {}

pub struct ServerAuthenticator {
    setup: ServerSetup<CS>,
    registration: ServerRegistration<CS>,
}

impl ServerAuthenticator {
    pub fn from_password(pw: &[u8]) -> Self {
        let mut rng = OsRng;
        let client_start = ClientRegistration::start(&mut rng, pw).unwrap();
        let setup = ServerSetup::new(&mut rng);
        let cred_id = b"";
        let server_start =
            ServerRegistration::start(&setup, client_start.message, cred_id).unwrap();
        let client_finish = client_start
            .state
            .finish(&mut rng, pw, server_start.message, Default::default())
            .unwrap();
        let registration = ServerRegistration::finish(client_finish.message);
        Self {
            setup,
            registration,
        }
    }

    pub async fn login<T: AsyncRead + AsyncWrite>(
        &self,
        client: &mut T,
    ) -> Result<(), AuthenticationError> {
        Ok(())
    }
}

pub async fn login<T: AsyncRead + AsyncWrite>(
    server: &mut T,
    pw: &[u8],
) -> Result<(), AuthenticationError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authenticator_from_password() {
        let _auth = ServerAuthenticator::from_password(b"deadbeef");
    }

    #[tokio::test]
    async fn authenticate_correct() {
        let password = b"deadbeef";
        let auth = ServerAuthenticator::from_password(password);
        let (mut client, mut server) = tokio::io::duplex(128);
        let server_result = tokio::spawn(async move { auth.login(&mut client).await })
            .await
            .unwrap();
        let client_result = login(&mut server, password).await;
        let server_secret = server_result.unwrap();
        let client_secret = client_result.unwrap();
        assert_eq!(server_secret, client_secret);
    }

    #[tokio::test]
    async fn authenticate_incorrect() {
        let password = b"deadbeef";
        let wrong_password = b"bingus_love";
        let auth = ServerAuthenticator::from_password(password);
        let (mut client, mut server) = tokio::io::duplex(128);
        let server_result = tokio::spawn(async move { auth.login(&mut client).await })
            .await
            .unwrap();
        let client_result = login(&mut server, wrong_password).await;
        assert!(!server_result.is_ok());
        assert!(!client_result.is_ok());
    }
}
