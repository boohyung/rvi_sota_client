use rustc_serialize::json;

use datatype::{AccessToken, AuthConfig, ClientId, ClientSecret, Error};
use http_client::{Auth, HttpClient, HttpRequest};


pub fn authenticate(config: &AuthConfig, client: &mut HttpClient) -> Result<AccessToken, Error> {

    debug!("authenticate()");

    let req = HttpRequest::post::<_, _, String>(
        config.server.join("/token").unwrap(),
        Some(Auth::Credentials(
            ClientId     { get: config.client_id.clone() },
            ClientSecret { get: config.secret.clone() })),
        None,
    );

    let resp = try!(client.send_request(&req));

    debug!("authenticate, body: `{}`", resp.body);

    Ok(try!(json::decode(&resp.body)))

}


#[cfg(test)]
mod tests {

    use super::*;
    use datatype::{AccessToken, AuthConfig};
    use http_client::TestHttpClient;

    const TOKEN: &'static str =
        r#"{"access_token": "token",
           "token_type": "type",
           "expires_in": 10,
           "scope": ["scope"]}
        "#;

    #[test]
    fn test_authenticate() {
        assert_eq!(authenticate(&AuthConfig::default(), &mut TestHttpClient::from(vec![TOKEN])).unwrap(),
                   AccessToken {
                       access_token: "token".to_string(),
                       token_type: "type".to_string(),
                       expires_in: 10,
                       scope: vec!["scope".to_string()]
                   })
    }

    #[test]
    fn test_authenticate_no_token() {
        assert_eq!(format!("{}", authenticate(&AuthConfig::default(),
                                              &mut TestHttpClient::from(vec![""])).unwrap_err()),
                   r#"Failed to decode JSON: ParseError(SyntaxError("EOF While parsing value", 1, 1))"#)

                   // XXX: Old error message was arguebly a lot better...
                   // "Authentication error, didn't receive access token.")
    }

    #[test]
    fn test_authenticate_bad_json() {
        assert_eq!(format!("{}", authenticate(&AuthConfig::default(),
                                              &mut TestHttpClient::from(vec![r#"{"apa": 1}"#])).unwrap_err()),
                   r#"Failed to decode JSON: MissingFieldError("access_token")"#)
    }

}
