use crate::app::Service;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resource {
    pub service: Service,
    pub kind: String,
    pub id: String,
    pub display: Vec<(String, String)>,
    pub details: Vec<(String, String)>,
    pub links: Vec<ResourceLink>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceLink {
    pub service: Service,
    pub kind: String,
    pub id: String,
}

pub struct Page<T> {
    pub items: Vec<T>,
    pub next_token: Option<String>,
}

// Implementations must expose specific reviewed SDK read calls, never an operation name.
pub trait ServiceReadAdapter {
    type Item;
    fn list_page(
        &self,
        token: Option<&str>,
    ) -> impl std::future::Future<Output = Result<Page<Self::Item>, String>> + Send;
}

pub async fn read_pages<A: ServiceReadAdapter>(
    adapter: &A,
    mut on_page: impl FnMut(Vec<A::Item>, bool),
) -> Result<(), String> {
    let mut token = None;
    loop {
        let page = adapter.list_page(token.as_deref()).await?;
        let done = page.next_token.as_deref().is_none_or(str::is_empty);
        on_page(page.items, done);
        if done {
            return Ok(());
        }
        token = page.next_token;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TwoPages;

    impl ServiceReadAdapter for TwoPages {
        type Item = &'static str;

        async fn list_page(&self, token: Option<&str>) -> Result<Page<Self::Item>, String> {
            match token {
                None => Ok(Page {
                    items: vec!["first"],
                    next_token: Some("next".into()),
                }),
                Some("next") => Ok(Page {
                    items: vec!["second"],
                    next_token: None,
                }),
                _ => Err("invalid token".into()),
            }
        }
    }

    #[tokio::test]
    async fn pages_are_sequential_and_marked_partial_until_done() {
        let mut seen = vec![];
        read_pages(&TwoPages, |rows, done| seen.push((rows, done)))
            .await
            .unwrap();
        assert_eq!(seen, vec![(vec!["first"], false), (vec!["second"], true)]);
    }

    struct EmptyThenError;

    impl ServiceReadAdapter for EmptyThenError {
        type Item = &'static str;

        async fn list_page(&self, token: Option<&str>) -> Result<Page<Self::Item>, String> {
            if token.is_none() {
                Ok(Page {
                    items: vec![],
                    next_token: Some("again".into()),
                })
            } else {
                Err("access denied".into())
            }
        }
    }

    #[tokio::test]
    async fn empty_partial_page_can_fail_without_claiming_completion() {
        let mut seen = vec![];
        let error = read_pages(&EmptyThenError, |items, done| seen.push((items, done))).await;
        assert_eq!(seen, vec![(vec![], false)]);
        assert_eq!(error.unwrap_err(), "access denied");
    }
}
