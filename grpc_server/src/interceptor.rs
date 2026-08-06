use tonic::{Request, Status};

#[derive(Clone)]
pub struct ApiKeyInterceptor {
    valid_keys: Vec<String>,
}

impl ApiKeyInterceptor {
    pub fn new(valid_keys: Vec<String>) -> Self {
        Self { valid_keys }
    }

    pub fn interceptor(self) -> impl FnMut(Request<()>) -> Result<Request<()>, Status> + Clone {
        move |req: Request<()>| {
            if self.valid_keys.is_empty() {
                return Ok(req);
            }

            match req.metadata().get("x-api-key") {
                Some(key) => {
                    let key_str = key.to_str().unwrap_or("");
                    if self.valid_keys.iter().any(|k| k == key_str) {
                        Ok(req)
                    } else {
                        Err(Status::unauthenticated("Invalid API key"))
                    }
                }
                None => Err(Status::unauthenticated("Missing x-api-key header")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tonic::metadata::MetadataValue;

    #[test]
    fn empty_keys_allows_all() {
        let interceptor = ApiKeyInterceptor::new(vec![]);
        let mut f = interceptor.interceptor();
        let req = Request::new(());
        assert!(f(req).is_ok());
    }

    #[test]
    fn missing_key_rejected() {
        let interceptor = ApiKeyInterceptor::new(vec!["secret".to_string()]);
        let mut f = interceptor.interceptor();
        let req = Request::new(());
        assert!(f(req).is_err());
    }

    #[test]
    fn valid_key_accepted() {
        let interceptor = ApiKeyInterceptor::new(vec!["secret".to_string()]);
        let mut f = interceptor.interceptor();
        let mut req = Request::new(());
        req.metadata_mut()
            .insert("x-api-key", "secret".parse::<MetadataValue<_>>().unwrap());
        assert!(f(req).is_ok());
    }

    #[test]
    fn invalid_key_rejected() {
        let interceptor = ApiKeyInterceptor::new(vec!["secret".to_string()]);
        let mut f = interceptor.interceptor();
        let mut req = Request::new(());
        req.metadata_mut()
            .insert("x-api-key", "wrong".parse::<MetadataValue<_>>().unwrap());
        assert!(f(req).is_err());
    }
}
