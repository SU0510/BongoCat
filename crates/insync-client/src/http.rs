//! The typed REST client. Every method maps server responses onto
//! `insync-core` types; idempotent retries are the sync engine's job.

use crate::error::{ClientError, error_from_response};
use insync_core::api::*;
use insync_core::{
    CoupleRelation, EventFrame, FocusSession, Letter, Message, Overview, PokeRecord, Presence,
    PresenceState, PublicUser, SharedAnniversary, SharedNote, SharedTodo,
};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct InSyncClient {
    base: String,
    token: Option<String>,
    http: reqwest::Client,
}

/// The 409 payload a stale note patch returns.
#[derive(Debug, Clone, PartialEq)]
pub struct NoteConflictError {
    pub current: SharedNote,
}

fn send_path(base: &str, path: &str) -> String {
    format!("{}{}", base.trim_end_matches('/'), path)
}

impl InSyncClient {
    pub fn new(base: &str) -> Self {
        Self {
            base: base.to_string(),
            token: None,
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .expect("reqwest client with valid config"),
        }
    }

    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    pub fn token(&self) -> Option<&str> {
        self.token.as_deref()
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    async fn request(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<reqwest::Response, ClientError> {
        let mut request = self.http.request(method, send_path(&self.base, path));
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        let status = response.status().as_u16();
        if !response.status().is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(error_from_response(status, &text));
        }
        Ok(response)
    }

    async fn get_json<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, ClientError> {
        let response = self.request(reqwest::Method::GET, path, None).await?;
        response
            .json()
            .await
            .map_err(|e| ClientError::Network(format!("decode {path}: {e}")))
    }

    async fn post_json<B: serde::Serialize, T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, ClientError> {
        let value =
            serde_json::to_value(body).map_err(|e| ClientError::Validation(e.to_string()))?;
        let response = self
            .request(reqwest::Method::POST, path, Some(&value))
            .await?;
        response
            .json()
            .await
            .map_err(|e| ClientError::Network(format!("decode {path}: {e}")))
    }

    async fn patch_json<B: serde::Serialize, T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, ClientError> {
        let value =
            serde_json::to_value(body).map_err(|e| ClientError::Validation(e.to_string()))?;
        let response = self
            .request(reqwest::Method::PATCH, path, Some(&value))
            .await?;
        response
            .json()
            .await
            .map_err(|e| ClientError::Network(format!("decode {path}: {e}")))
    }

    async fn delete(&self, path: &str) -> Result<(), ClientError> {
        self.request(reqwest::Method::DELETE, path, None).await?;
        Ok(())
    }

    async fn post_empty(&self, path: &str) -> Result<(), ClientError> {
        self.request(reqwest::Method::POST, path, None).await?;
        Ok(())
    }

    // ------------------------------------------------------------- auth

    pub async fn register(&mut self, req: &RegisterRequest) -> Result<PublicUser, ClientError> {
        let auth: AuthResponse = self.post_json("/v1/auth/register", req).await?;
        self.token = Some(auth.token);
        Ok(auth.user)
    }

    pub async fn login(&mut self, req: &AuthRequest) -> Result<PublicUser, ClientError> {
        let auth: AuthResponse = self.post_json("/v1/auth/login", req).await?;
        self.token = Some(auth.token);
        Ok(auth.user)
    }

    pub async fn set_token(&mut self, token: impl Into<String>) {
        self.token = Some(token.into());
    }

    pub async fn logout(&mut self) -> Result<(), ClientError> {
        let result = self.post_empty("/v1/auth/logout").await;
        self.token = None;
        result
    }

    // ------------------------------------------------------------- pairing

    pub async fn me(&self) -> Result<MeResponse, ClientError> {
        self.get_json("/v1/me").await
    }

    pub async fn create_invite(&self) -> Result<InviteResponse, ClientError> {
        self.post_json("/v1/pair/invite", &serde_json::json!({}))
            .await
    }

    pub async fn accept_invite(&self, code: &str) -> Result<PairResponse, ClientError> {
        self.post_json(
            "/v1/pair/accept",
            &AcceptInviteRequest {
                code: code.to_string(),
            },
        )
        .await
    }

    pub async fn dissolve(&self) -> Result<(), ClientError> {
        self.post_empty("/v1/pair/dissolve").await
    }

    // ------------------------------------------------------------- chat

    pub async fn send_message(&self, client_id: &str, body: &str) -> Result<Message, ClientError> {
        self.post_json(
            "/v1/messages",
            &SendMessageRequest {
                client_id: client_id.to_string(),
                body: body.to_string(),
            },
        )
        .await
    }

    pub async fn messages(&self, after_seq: u64) -> Result<MessagePage, ClientError> {
        self.get_json(&format!("/v1/messages?after_seq={after_seq}&limit=200"))
            .await
    }

    pub async fn mark_read(&self, last_message_seq: u64) -> Result<(), ClientError> {
        self.post_empty_body("/v1/messages/read", &ReadRequest { last_message_seq })
            .await
    }

    pub async fn send_letter(&self, client_id: &str, body: &str) -> Result<Letter, ClientError> {
        self.post_json(
            "/v1/letters",
            &SendLetterRequest {
                client_id: client_id.to_string(),
                body: body.to_string(),
            },
        )
        .await
    }

    pub async fn letters(&self) -> Result<Vec<Letter>, ClientError> {
        let page: LetterPage = self.get_json("/v1/letters").await?;
        Ok(page.letters)
    }

    pub async fn read_letter(&self, letter_id: &str) -> Result<(), ClientError> {
        self.post_empty(&format!("/v1/letters/{letter_id}/read"))
            .await
    }

    pub async fn delete_letter(&self, letter_id: &str) -> Result<(), ClientError> {
        self.delete(&format!("/v1/letters/{letter_id}")).await
    }

    /// Returns `Ok(None)` when the server's poke cooldown rejected the send.
    pub async fn send_poke(&self, client_id: &str) -> Result<Option<PokeRecord>, ClientError> {
        match self
            .post_json::<_, PokeRecord>(
                "/v1/pokes",
                &SendPokeRequest {
                    client_id: client_id.to_string(),
                },
            )
            .await
        {
            Ok(poke) => Ok(Some(poke)),
            Err(ClientError::RateLimited { .. }) => Ok(None),
            Err(error) => Err(error),
        }
    }

    // ------------------------------------------------------------- focus

    pub async fn focus_invite(
        &self,
        client_id: &str,
        topic: Option<String>,
        duration_secs: i64,
    ) -> Result<FocusSession, ClientError> {
        let response: FocusResponse = self
            .post_json(
                "/v1/focus/invite",
                &FocusInviteRequest {
                    client_id: client_id.to_string(),
                    topic,
                    duration_secs,
                },
            )
            .await?;
        Ok(response.focus)
    }

    pub async fn focus_action(
        &self,
        focus_id: &str,
        action: &str,
    ) -> Result<FocusSession, ClientError> {
        let response: FocusResponse = self
            .post_json(
                &format!("/v1/focus/{focus_id}/{action}"),
                &serde_json::json!({}),
            )
            .await?;
        Ok(response.focus)
    }

    // ------------------------------------------------------------- shared items

    pub async fn todos(&self) -> Result<Vec<SharedTodo>, ClientError> {
        Ok(self.get_json::<TodoPage>("/v1/todos").await?.todos)
    }

    pub async fn create_todo(&self, req: &CreateTodoRequest) -> Result<SharedTodo, ClientError> {
        self.post_json("/v1/todos", req).await
    }

    pub async fn update_todo(
        &self,
        todo_id: &str,
        req: &UpdateTodoRequest,
    ) -> Result<SharedTodo, ClientError> {
        self.patch_json(&format!("/v1/todos/{todo_id}"), req).await
    }

    pub async fn delete_todo(&self, todo_id: &str) -> Result<(), ClientError> {
        self.delete(&format!("/v1/todos/{todo_id}")).await
    }

    pub async fn notes(&self) -> Result<Vec<SharedNote>, ClientError> {
        Ok(self.get_json::<NotePage>("/v1/notes").await?.notes)
    }

    pub async fn create_note(&self, req: &CreateNoteRequest) -> Result<SharedNote, ClientError> {
        self.post_json("/v1/notes", req).await
    }

    pub async fn update_note(
        &self,
        note_id: &str,
        req: &UpdateNoteRequest,
    ) -> Result<Result<SharedNote, NoteConflictError>, ClientError> {
        match self
            .patch_json::<_, SharedNote>(&format!("/v1/notes/{note_id}"), req)
            .await
        {
            Ok(note) => Ok(Ok(note)),
            Err(ClientError::Conflict { .. }) => {
                // re-fetch with a plain GET to get the authoritative body
                // (the 409 body is not a JSON the typed decoder accepts)
                let conflict = self
                    .get_json::<NotePage>("/v1/notes")
                    .await?
                    .notes
                    .into_iter()
                    .find(|note| note.id == note_id);
                match conflict {
                    Some(current) => Ok(Err(NoteConflictError { current })),
                    None => Err(ClientError::NotFound(note_id.to_string())),
                }
            }
            Err(error) => Err(error),
        }
    }

    pub async fn delete_note(&self, note_id: &str) -> Result<(), ClientError> {
        self.delete(&format!("/v1/notes/{note_id}")).await
    }

    pub async fn anniversaries(&self) -> Result<Vec<SharedAnniversary>, ClientError> {
        Ok(self
            .get_json::<AnniversaryPage>("/v1/anniversaries")
            .await?
            .anniversaries)
    }

    pub async fn create_anniversary(
        &self,
        req: &CreateAnniversaryRequest,
    ) -> Result<SharedAnniversary, ClientError> {
        self.post_json("/v1/anniversaries", req).await
    }

    pub async fn update_anniversary(
        &self,
        anniversary_id: &str,
        req: &UpdateAnniversaryRequest,
    ) -> Result<SharedAnniversary, ClientError> {
        self.patch_json(&format!("/v1/anniversaries/{anniversary_id}"), req)
            .await
    }

    pub async fn delete_anniversary(&self, anniversary_id: &str) -> Result<(), ClientError> {
        self.delete(&format!("/v1/anniversaries/{anniversary_id}"))
            .await
    }

    // ------------------------------------------------------------- presence & events

    pub async fn set_presence(&self, state: PresenceState) -> Result<(), ClientError> {
        self.post_empty_body("/v1/presence", &PresenceRequest { state })
            .await
    }

    pub async fn partner_presence(&self) -> Result<Option<Presence>, ClientError> {
        self.get_json("/v1/presence").await
    }

    pub async fn overview(&self) -> Result<Overview, ClientError> {
        Ok(self
            .get_json::<OverviewResponse>("/v1/overview")
            .await?
            .overview)
    }

    pub async fn events(&self, after: u64) -> Result<(Vec<EventFrame>, u64), ClientError> {
        let page: EventPage = self
            .get_json(&format!("/v1/events?after={after}&limit=500"))
            .await?;
        Ok((page.events, page.latest_seq))
    }

    pub async fn relation(&self) -> Result<Option<CoupleRelation>, ClientError> {
        Ok(self.me().await?.relation)
    }

    async fn post_empty_body<B: serde::Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<(), ClientError> {
        let value =
            serde_json::to_value(body).map_err(|e| ClientError::Validation(e.to_string()))?;
        self.request(reqwest::Method::POST, path, Some(&value))
            .await?;
        Ok(())
    }
}
