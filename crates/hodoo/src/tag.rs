//! `project.tags`.
//!
//! Tags are shared across projects, not owned by one, so tagging a task is
//! "find this tag or make it".

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::Client;
use crate::error::{Error, Result};
use crate::id::TagId;
use crate::query;

/// The fields this crate reads from `project.tags`.
pub const FIELDS: &[&str] = &["id", "name", "color"];

/// A tag.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tag {
    /// `project.tags.id`.
    pub id: TagId,
    /// Tag name.
    #[serde(default)]
    pub name: String,
    /// Colour index.
    #[serde(default)]
    pub color: i64,
}

/// Access to `project.tags`.
#[derive(Debug, Clone, Copy)]
pub struct Tags<'a> {
    client: &'a Client,
}

impl<'a> Tags<'a> {
    pub(crate) fn new(client: &'a Client) -> Self {
        Self { client }
    }

    /// Lists every tag, by name.
    ///
    /// # Errors
    ///
    /// Any error of [`Client::call`], plus [`Error::Decode`] if a record does
    /// not match [`Tag`].
    pub async fn list(&self) -> Result<Vec<Tag>> {
        self.client
            .call_as(
                "project.tags",
                "search_read",
                query::search(json!([]), FIELDS, None, None, Some("name")),
            )
            .await
    }

    /// The id of a tag by name, ignoring case, or `None` when there is none.
    ///
    /// Searching without creating is what a filter or a dry run needs: neither may
    /// write to the server.
    ///
    /// # Errors
    ///
    /// Any error of [`Client::call`].
    pub async fn find(&self, name: &str) -> Result<Option<TagId>> {
        // `=ilike` rather than `=`: `--tag Urgent` meaning a second tag next to
        // `urgent` is a duplicate nobody asked for. Its pattern characters are
        // escaped so a tag called `50%` does not match `50 anything`.
        let pattern = name
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        let domain: Value = json!([["name", "=ilike", pattern]]);
        let existing: Vec<Tag> = self
            .client
            .call_as(
                "project.tags",
                "search_read",
                query::search(domain, FIELDS, Some(1), None, None),
            )
            .await?;
        Ok(existing.into_iter().next().map(|tag| tag.id))
    }

    /// The id of a tag by name (ignoring case), creating it when it does not exist yet.
    ///
    /// # Errors
    ///
    /// Any error of [`Client::call`].
    pub async fn ensure(&self, name: &str) -> Result<TagId> {
        if let Some(id) = self.find(name).await? {
            return Ok(id);
        }

        let created = self
            .client
            .call("project.tags", "name_create", json!({ "name": name }))
            .await?;
        query::created_id(&created)
            .map(TagId::new)
            .ok_or_else(|| Error::UnexpectedResponse {
                status: 200,
                body: Error::truncate(created.to_string()),
            })
    }
}
