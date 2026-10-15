use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use chrono::{DateTime, Utc};
use exif::{Tag, Value};
use log::{debug, error, warn};
use reqwest::{Client, Url, header::HeaderMap};
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::folders::ConflictResolution;
use crate::urls;

#[derive(Debug, Serialize)]
struct QueryNodeCreateNode {
    kind: String,
    name: String,
    #[serde(rename(serialize = "resourceVersion"))]
    resource_version: String,
    #[serde(rename(serialize = "contentType"))]
    content_type: String,
}

#[derive(Debug, Serialize)]
struct QueryNodeModify {
    op: String,
    value: Vec<String>,
    #[serde(rename(serialize = "resourceVersion"))]
    resource_version: String,
    #[serde(rename(serialize = "contentType"))]
    content_type: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct ResponseAccessToken {
    access_token: String,
    duration: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ResponseEmpty {}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ResponseMatch {
    count: u32,
    r#match: String,
    #[serde(rename(deserialize = "searchData"))]
    search_data: ResponseEmpty,
}

#[derive(Debug, Deserialize, Serialize)]
struct ResponseAggregations {
    #[serde(rename(deserialize = "allPeople"))]
    all_people: Vec<String>,
    #[serde(rename(deserialize = "clusterId"))]
    cluster_id: Vec<String>,
    favorite: Vec<String>,
    location: Vec<String>,
    people: Vec<String>,
    things: Vec<String>,
    time: Vec<ResponseMatch>,
    r#type: Vec<ResponseMatch>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ResponsePicture {
    id: String,
    #[serde(rename(deserialize = "isDefault"))]
    is_default: bool,
    #[serde(rename(deserialize = "ownerId"))]
    owner_id: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct ResponseCollectionProperties {
    covers: Vec<ResponsePicture>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ResponseSearchData {
    #[serde(rename(deserialize = "accessRuleIds"))]
    access_rule_ids: Vec<String>,
    #[serde(rename(deserialize = "childAssetTypeInfo"))]
    child_asset_type_info: Vec<String>,
    #[serde(default, rename(deserialize = "collectionProperties"))]
    collection_properties: ResponseCollectionProperties,
    #[serde(rename(deserialize = "createdBy"))]
    created_by: String,
    #[serde(rename(deserialize = "createdDate"))]
    created_date: DateTime<Utc>,
    #[serde(default, rename(deserialize = "eTagResponse"))]
    e_tag_response: String,
    #[serde(rename(deserialize = "groupPermissions"))]
    group_permissions: Vec<String>,
    pub id: String,
    #[serde(rename(deserialize = "isRoot"))]
    is_root: bool,
    #[serde(rename(deserialize = "isShared"))]
    is_shared: bool,
    keywords: Vec<String>,
    kind: String,
    labels: Vec<String>,
    #[serde(rename(deserialize = "modifiedDate"))]
    modified_date: DateTime<Utc>,
    #[serde(default)]
    pub name: String,
    #[serde(rename(deserialize = "ownerId"))]
    pub owner_id: String,
    #[serde(rename(deserialize = "parentMap"))]
    parent_map: ResponseEmpty,
    parents: Vec<String>,
    #[serde(rename(deserialize = "protectedFolder"))]
    protected_folder: bool,
    restricted: bool,
    status: String,
    #[serde(rename(deserialize = "subKinds"))]
    sub_kinds: Vec<String>,
    transforms: Vec<String>,
    version: u16,
    #[serde(rename(deserialize = "xAccntParentMap"))]
    x_accnt_parent_map: ResponseEmpty,
    #[serde(rename(deserialize = "xAccntParents"))]
    x_accnt_parents: Vec<String>,
}

impl ResponseSearchData {
    pub fn empty() -> ResponseSearchData {
        let empty_vec: Vec<String> = Vec::new();
        let empty_str: String = "".to_string();
        let empty_chrono: chrono::DateTime<Utc> = chrono::DateTime::<Utc>::from_timestamp_nanos(0);
        let empty_resp = ResponseEmpty {};
        let empty_coll_props = ResponseCollectionProperties {
            ..Default::default()
        };
        return ResponseSearchData {
            access_rule_ids: empty_vec.clone(),
            child_asset_type_info: empty_vec.clone(),
            collection_properties: empty_coll_props,
            created_by: empty_str.clone(),
            created_date: empty_chrono,
            e_tag_response: empty_str.clone(),
            group_permissions: empty_vec.clone(),
            id: empty_str.clone(),
            is_root: false,
            is_shared: false,
            keywords: empty_vec.clone(),
            kind: empty_str.clone(),
            labels: empty_vec.clone(),
            modified_date: empty_chrono,
            name: empty_str.clone(),
            owner_id: empty_str.clone(),
            parent_map: empty_resp.clone(),
            parents: empty_vec.clone(),
            protected_folder: false,
            restricted: false,
            status: empty_str.clone(),
            sub_kinds: empty_vec.clone(),
            transforms: empty_vec.clone(),
            version: 0,
            x_accnt_parent_map: empty_resp.clone(),
            x_accnt_parents: empty_vec.clone(),
        };
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ResponseSearch {
    aggregations: ResponseAggregations,
    pub count: u32,
    pub data: Vec<ResponseSearchData>,
    #[serde(rename(deserialize = "nodeToSearchScoreMap"))]
    node_to_search_score_map: ResponseEmpty,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ResponseNode {
    #[serde(rename(deserialize = "FOLDER"))]
    folder: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ResponseNodes {
    count: u32,
    data: Vec<ResponseSearchData>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ResponseImageProperties {
    #[serde(default)]
    orientation: Option<String>,
    #[serde(rename(deserialize = "resolutionUnit"))]
    resolution_unit: String,
    width: u32,
    height: u32,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ResponseContentSignature {
    #[serde(rename(deserialize = "contentSignatureType"))]
    content_signature_type: String,
    #[serde(rename(deserialize = "contentSignature"))]
    content_signature: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ResponseContentProperties {
    size: usize,
    version: u16,
    #[serde(rename(deserialize = "contentType"))]
    content_type: String,
    extension: String,
    md5: String,
    image: ResponseImageProperties,
    #[serde(rename(deserialize = "contentSignatures"))]
    content_signatures: Vec<ResponseContentSignature>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ResponseUpload {
    pub id: String,
    name: String,
    kind: String,
    version: u16,
    #[serde(rename(deserialize = "modifiedDate"))]
    modified_date: DateTime<Utc>,
    #[serde(rename(deserialize = "createdDate"))]
    created_date: DateTime<Utc>,
    parents: Vec<String>,
    #[serde(rename(deserialize = "parentMap"))]
    parent_map: ResponseNode,
    status: String,
    restricted: bool,
    #[serde(rename(deserialize = "protectedFolder"))]
    protected_folder: bool,
    #[serde(rename(deserialize = "contentProperties"))]
    content_properties: ResponseContentProperties,
    properties: ResponseEmpty,
    transforms: Vec<String>,
    keywords: Vec<String>,
    #[serde(rename(deserialize = "ownerId"))]
    owner_id: String,
    #[serde(rename(deserialize = "subKinds"))]
    sub_kinds: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ResponseUploadFail {
    message: String,
    #[serde(rename(deserialize = "errorCode"))]
    error_code: String,
    // Get timestamp as string because of some parsing issues
    #[serde(rename(deserialize = "timeStamp"))]
    time_stamp: String,
    #[serde(rename(deserialize = "requestId"))]
    request_id: String,
    #[serde(rename(deserialize = "errorDetails"))]
    error_details: ResponseUploadFailDetails,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ResponseUploadFailDetails {
    #[serde(rename(deserialize = "conflictNodeIds"))]
    conflict_node_ids: Vec<String>,
}

/// Bounded retry policy for the upload POST (transient failures only):
/// 1 initial attempt + 3 retries.
const UPLOAD_MAX_ATTEMPTS: u32 = 4;
/// First backoff step (doubles per attempt, plus jitter).
const UPLOAD_BACKOFF_BASE_MS: u64 = 500;
/// Upper bound for any single backoff / `Retry-After` sleep.
const UPLOAD_BACKOFF_MAX_MS: u64 = 8_000;

/// Marker prefix for an upload conflict reported by the upload endpoint (HTTP 409,
/// `docs/SPEC_DEDUP.md` §4.1). The upload worker resolves it per the folder's
/// policy — skip for `Ignore`, ask the user per file for `Ask`.
pub(crate) const CONFLICT_PREFIX: &str = "\u{1}conflict\u{1}";
/// Separator between the server's `errorCode` and the node id inside the
/// marker. The whole marker is a transient `Err(String)` from `upload_picture`
/// and is never persisted, so the format can carry both parts safely.
const CONFLICT_CAUSE_SEP: char = '\u{2}';

/// Build the conflict marker carrying the server's `errorCode` (the 409 cause,
/// e.g. `DuplicatesConflictError` or `NameAlreadyExists`) and the id of the
/// node already on the server (empty when the response named none).
pub(crate) fn conflict_error(code: &str, existing_node_id: &str) -> String {
    format!("{CONFLICT_PREFIX}{code}{CONFLICT_CAUSE_SEP}{existing_node_id}")
}

/// `Some(existing_node_id)` when `err` is a conflict marker, else `None`.
pub(crate) fn conflict_node_id(err: &str) -> Option<&str> {
    let rest = err.strip_prefix(CONFLICT_PREFIX)?;
    Some(match rest.split_once(CONFLICT_CAUSE_SEP) {
        Some((_, id)) => id,
        // No cause separator: the whole remainder is the node id.
        None => rest,
    })
}

/// The 409 cause (the response's `errorCode`) when `err` is a conflict
/// marker, else `None`. The code is empty when the marker carries none.
pub(crate) fn conflict_cause(err: &str) -> Option<&str> {
    let rest = err.strip_prefix(CONFLICT_PREFIX)?;
    Some(match rest.split_once(CONFLICT_CAUSE_SEP) {
        Some((code, _)) => code,
        None => "",
    })
}

/// True when a 409 `errorCode` means "the file's content is already in the
/// account" (an MD5 duplicate, possibly under a different path) — as opposed
/// to a name conflict where the content is *not* there. Compared
/// case-insensitively: the vocabulary appears both as
/// `DuplicatesConflictError`/`MD5_DUPLICATE` and in other spellings. Anything
/// unknown or empty counts as *no* content duplicate.
pub(crate) fn is_content_duplicate_code(code: &str) -> bool {
    let code = code.trim();
    code.eq_ignore_ascii_case("DuplicatesConflictError")
        || code.eq_ignore_ascii_case("MD5_DUPLICATE")
}

/// Flatten an error together with its whole `source()` chain into one line, so
/// the underlying cause (DNS failure, connection reset, body stream, timeout)
/// is not swallowed behind reqwest's generic "error sending request for url".
fn error_chain(e: &dyn std::error::Error) -> String {
    let mut parts = vec![e.to_string()];
    let mut source = e.source();
    while let Some(inner) = source {
        parts.push(inner.to_string());
        source = inner.source();
    }
    parts.join(": ")
}

/// Transient reqwest failures worth a retry: connect/timeout/request/body.
/// (Retry on a response status is decided separately from the response itself.)
fn is_transient_reqwest_error(e: &reqwest::Error) -> bool {
    e.is_connect() || e.is_timeout() || e.is_request() || e.is_body()
}

/// Small pseudo-random jitter in `0..max_ms`, derived from the wall clock, so no
/// extra dependency (rand et al.) is needed.
fn jitter_millis(max_ms: u64) -> u64 {
    if max_ms == 0 {
        return 0;
    }
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64 % max_ms)
        .unwrap_or(0)
}

/// `Retry-After` (delta-seconds form, as sent on 429/503) capped like the
/// backoff so a huge value cannot stall an upload. Absent values and the
/// HTTP-date form fall back to the exponential backoff.
fn retry_after_delay(headers: &HeaderMap) -> Option<Duration> {
    let seconds: u64 = headers
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse()
        .ok()?;
    Some(Duration::from_millis(
        seconds.saturating_mul(1000).clamp(1, UPLOAD_BACKOFF_MAX_MS),
    ))
}

/// Backoff before the retry that follows `attempt` (1-based): exponential
/// (base ~500 ms, ×2) with small jitter, capped at ~8 s. A `Retry-After` header
/// takes precedence when present.
fn upload_retry_delay(attempt: u32, headers: Option<&HeaderMap>) -> Duration {
    if let Some(delay) = headers.and_then(retry_after_delay) {
        return delay;
    }
    let shift = attempt.saturating_sub(1).min(10);
    let base_ms = UPLOAD_BACKOFF_BASE_MS
        .saturating_mul(1u64 << shift)
        .min(UPLOAD_BACKOFF_MAX_MS);
    let jittered = (base_ms + jitter_millis(base_ms / 4 + 1)).min(UPLOAD_BACKOFF_MAX_MS);
    Duration::from_millis(jittered)
}

/// Account preferences as answered by `GET /drive/v1/account/preferences`.
/// Only the two fields this feature needs; every other field is ignored, and a
/// missing/absent field deserialises to the empty/`None` default instead of
/// failing the whole parse.
#[derive(Debug, Deserialize, Serialize)]
struct ResponsePreferences {
    #[serde(default)]
    name: String,
    #[serde(default, rename(deserialize = "avatarNodeId"))]
    avatar_node_id: Option<String>,
}

/// Avatar view box in pixels (the original's `AvatarViewBoxSize = 60`, see
/// `docs/SPEC_ACCOUNT_BILLING.md` §3.4). Also used by
/// [`crate::urls::thumbnail_url`].
pub(crate) const AVATAR_VIEW_BOX: u32 = 60;

/// Local cache file name for an avatar thumbnail: `<nodeId>_60_60.jpeg`
/// (the original's `ThumbnailPath/<nodeId>_60_60.jpeg`).
pub(crate) fn avatar_cache_file_name(node_id: &str) -> String {
    format!("{node_id}_{}_{}.jpeg", AVATAR_VIEW_BOX, AVATAR_VIEW_BOX)
}

/// Cache path for an avatar thumbnail inside a given base directory (`thumbnails/`).
/// Pure, so the layout is unit-testable without touching the real app dir.
pub(crate) fn avatar_cache_path_in(base: &Path, node_id: &str) -> PathBuf {
    base.join("thumbnails")
        .join(avatar_cache_file_name(node_id))
}

/// Cache path under the app data dir — the same directory that holds
/// `config.toml` / `dedup.sqlite3`. `None` when no config dir is available.
pub(crate) fn avatar_cache_path(node_id: &str) -> Option<PathBuf> {
    let base = dirs::config_dir()?.join("prime_photos");
    Some(avatar_cache_path_in(&base, node_id))
}

/// True when `bytes` begin with a JPEG or PNG signature. A cached file is only
/// trusted when it does; anything else counts as corrupt and is refetched.
pub(crate) fn looks_like_image(bytes: &[u8]) -> bool {
    const PNG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.starts_with(&[0xFF, 0xD8, 0xFF]) || bytes.starts_with(&PNG)
}

#[derive(Debug)]
pub struct PrimePhotos {
    conf: Config,
    web_dir_node: Option<ResponseSearchData>,
    client: Client,
    dry_run: bool,
}

impl PrimePhotos {
    pub fn new(conf: &Config, dry_run: bool) -> Self {
        // Configure client with connection pooling and optimized settings
        let client = Client::builder()
            .pool_max_idle_per_host(10)
            .pool_idle_timeout(std::time::Duration::from_secs(90))
            .tcp_keepalive(std::time::Duration::from_secs(60))
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .unwrap_or_else(|_| Client::new());

        Self {
            conf: conf.clone(),
            web_dir_node: None,
            client,
            dry_run: dry_run,
        }
    }

    fn get_epoch_millis() -> String {
        let systime = SystemTime::now();
        let since_epoch = systime
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards");
        since_epoch.as_millis().to_string()
    }

    /// Build common headers for API requests (cached to avoid rebuilding)
    fn build_common_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("Content-Type", "application/json".parse().unwrap());
        headers.insert(
            "Accept",
            "application/json, text/javascript, */*; q=0.01"
                .parse()
                .unwrap(),
        );
        headers.insert("User-Agent", self.conf.user_agent.parse().unwrap());
        headers.insert(
            "Cookie",
            format!(
                "session-id={}; x-acb{}={}; at-acb{}={}; ubid-acb{}={}",
                self.conf.session_id,
                self.conf.country,
                self.conf.cookie_x_acb,
                self.conf.country,
                self.conf.cookie_at_acb,
                self.conf.country,
                self.conf.cookie_ubid_acb
            )
            .parse()
            .unwrap(),
        );
        headers.insert("x-amzn-SessionId", self.conf.session_id.parse().unwrap());
        headers
    }

    /// Check if the response is a 401 Unauthorized error and provide helpful message
    /// Returns true if it was a 401 error (and prints the helpful message)
    fn check_auth_error(status_code: u16) -> bool {
        if status_code == 401 {
            error!("Authentication failed: Your session cookies have expired.");
            error!("");
            error!("Please update your cookies using the config_update.py script:");
            error!("  1. Open Amazon Photos in your web browser and log in");
            error!("  2. Open Developer Tools (F12)");
            error!("  3. In the Console tab, run: document.cookie");
            error!("  4. Copy the entire cookie string");
            error!("  5. Run: python3 config_update.py 'session-id=...; x-acb...=...; ...'");
            error!("");
            return true;
        }
        false
    }

    /// Get an auth token valid across all Amazon boundaries - Used for content
    /// uploading. Called by the upload worker when a run hits a 401: the session
    /// cookies are still valid, only this token went stale.
    pub(crate) async fn renew_auth_token(&mut self) -> Result<&str, reqwest::Error> {
        let url = Url::parse(&urls::auth_token_url(&self.conf.country))
            .expect("Can't parse the token renewal URL");

        // Provide timestamp as a parameter
        let params = [("_", PrimePhotos::get_epoch_millis())];
        let mut headers = HeaderMap::new();
        headers.append("Content-Type", "application/json".parse().unwrap());
        headers.append(
            "Accept",
            "application/json, text/javascript, */*; q=0.01"
                .parse()
                .unwrap(),
        );
        headers.append("User-Agent", self.conf.user_agent.parse().unwrap());
        headers.append(
            "Cookie",
            format!(
                "session-id={}; x-acb{}={}; at-acb{}={}; ubid-acb{}={}",
                self.conf.session_id,
                self.conf.country,
                self.conf.cookie_x_acb,
                self.conf.country,
                self.conf.cookie_at_acb,
                self.conf.country,
                self.conf.cookie_ubid_acb
            )
            .parse()
            .unwrap(),
        );
        headers.append("x-amzn-SessionId", self.conf.session_id.parse().unwrap());

        // Stop early
        if self.dry_run {
            debug!("Dry-run, give back fake data");
            return Ok("");
        }

        // Submit the GET request
        let response = self
            .client
            .get(url)
            .headers(headers)
            .query(&params)
            .send()
            .await?;

        // Check the HTTP status
        if response.status().is_success() {
            let body = response.json::<ResponseAccessToken>().await?;
            // debug!(
            //     "Access token, valid {} seconds:{}",
            //     body.duration, body.access_token
            // );
            self.conf.cookie_x_amz_access_token = body.access_token;
            Ok(&self.conf.cookie_x_amz_access_token)
        } else {
            let status_code = response.status().as_u16();
            debug!("Request failed with status: {}", status_code);
            Self::check_auth_error(status_code);
            Err(response.error_for_status().err().unwrap())
        }
    }

    async fn list_albums(&self) -> Result<ResponseSearch, reqwest::Error> {
        self.search(format!("?asset=ALL&filters=type%3A(ALBUMS)&limit=200&lowResThumbnail=true&searchContext=customer&sort=%5B%27createdDate+DESC%27%5D&tempLink=false&resourceVersion=V2&ContentType=JSON&_={}", PrimePhotos::get_epoch_millis()).as_str()).await
    }

    async fn create_album(&self, name: &str) -> Result<ResponseSearchData, reqwest::Error> {
        let url = Url::parse(&urls::nodes_url(&self.conf.country))
            .expect("Can't parse the create album URL");

        let headers = self.build_common_headers();
        let payload = QueryNodeCreateNode {
            kind: "VISUAL_COLLECTION".to_string(),
            name: name.to_string(),
            resource_version: "V2".to_string(),
            content_type: "JSON".to_string(),
        };

        // Return early
        if self.dry_run {
            debug!("Dry-run, return fake data");
            return Ok(ResponseSearchData::empty());
        }

        // Submit the POST request
        let response = self
            .client
            .post(url)
            .headers(headers)
            .json(&payload)
            .send()
            .await?;

        // Check the HTTP status
        if response.status().is_success() {
            let body = response.json::<ResponseSearchData>().await?;
            Ok(body)
        } else {
            let status_code = response.status().as_u16();
            debug!("Request failed with status: {}", status_code);
            Self::check_auth_error(status_code);
            Err(response.error_for_status().err().unwrap())
        }
    }

    /// Get a reference to an album by name, create it if needed
    pub async fn get_album(&self, name: &str) -> Result<ResponseSearchData, reqwest::Error> {
        let existing_albums = self.list_albums().await?;
        debug!("Got {} existing albums:", existing_albums.count);
        for album in existing_albums.data {
            debug!("\t{}", album.name);
            if album.name == name {
                return Ok(album);
            }
        }

        self.create_album(name).await
    }

    async fn search(&self, query: &str) -> Result<ResponseSearch, reqwest::Error> {
        let url = Url::parse(&urls::search_url(&self.conf.country, query))
            .expect("Can't parse the search URL");

        let headers = self.build_common_headers();

        // Return early
        if self.dry_run {
            debug!("Dry run, return fake data");
            let empty_vec_str: Vec<String> = Vec::new();
            let empty_vec_respmatch: Vec<ResponseMatch> = Vec::new();
            let empty_vec_respdata: Vec<ResponseSearchData> = Vec::new();
            return Ok(ResponseSearch {
                count: 0,
                aggregations: ResponseAggregations {
                    all_people: empty_vec_str.clone(),
                    cluster_id: empty_vec_str.clone(),
                    favorite: empty_vec_str.clone(),
                    location: empty_vec_str.clone(),
                    people: empty_vec_str.clone(),
                    things: empty_vec_str.clone(),
                    time: empty_vec_respmatch.clone(),
                    r#type: empty_vec_respmatch.clone(),
                },
                data: empty_vec_respdata,
                node_to_search_score_map: ResponseEmpty {},
            });
        }

        // Submit the GET request
        let response = self.client.get(url).headers(headers).send().await?;

        // Check the HTTP status
        if response.status().is_success() {
            let body = response.json::<ResponseSearch>().await?;
            debug!("Search response: {:?}", body);
            Ok(body)
        } else {
            let status_code = response.status().as_u16();
            debug!("Request failed with status: {}", status_code);
            Self::check_auth_error(status_code);
            Err(response.error_for_status().err().unwrap())
        }
    }

    async fn find_root_node(&self) -> Result<ResponseSearchData, String> {
        let nodes = self
            .find_nodes("isRoot:true")
            .await
            .map_err(|e| error_chain(&e))?;
        if nodes.data.is_empty() {
            error!("No root node found (empty response)");
            return Err("No root node found (empty response)".to_string());
        }
        Ok(nodes.data.first().unwrap().clone())
    }

    async fn update_web_pictures_dir_node(&mut self) -> Result<ResponseSearchData, String> {
        // Note: These operations must be sequential because each depends on the previous result
        // The Pictures directory is needed to find its children (Web directory)
        let root_node = self.find_root_node().await?;
        let pictures_dir_searchdata = self
            .find_children_nodes(
                "kind:FOLDER+AND+status:(AVAILABLE*)+AND+name:\"Pictures\"",
                &root_node.id,
            )
            .await
            .map_err(|e| error_chain(&e))?;
        let pictures_dir = pictures_dir_searchdata
            .data
            .first()
            .ok_or_else(|| "Pictures folder not found".to_string())?;
        let web_dir_node = self
            .find_children_nodes(
                "kind:FOLDER+AND+status:(AVAILABLE*)+AND+name:\"Web\"",
                &pictures_dir.id,
            )
            .await
            .map_err(|e| error_chain(&e))?
            .data
            .first()
            .cloned()
            .ok_or_else(|| "Web folder not found".to_string())?;
        self.web_dir_node = Some(web_dir_node.clone());
        Ok(web_dir_node)
    }

    async fn find_children_nodes(
        &self,
        filter: &str,
        node_id: &str,
    ) -> Result<ResponseNodes, reqwest::Error> {
        let url = Url::parse(&urls::children_nodes_url(
            &self.conf.country,
            node_id,
            filter,
            &PrimePhotos::get_epoch_millis(),
        ))
        .expect("Can't parse children nodes URL");
        let params = [];
        self.find_nodes_internal(url, &params).await
    }

    async fn find_nodes(&self, filter: &str) -> Result<ResponseNodes, reqwest::Error> {
        let url = Url::parse(&urls::nodes_base_url(&self.conf.country))
            .expect("Can't parse the find node URL");
        let epoch = PrimePhotos::get_epoch_millis();
        let params = [
            ("filters", filter),
            ("resourceVersion", "V2"),
            ("ContentType", "JSON"),
            ("_", epoch.as_str()),
        ];
        self.find_nodes_internal(url, &params).await
    }

    async fn find_nodes_internal(
        &self,
        url: Url,
        params: &[(&str, &str)],
    ) -> Result<ResponseNodes, reqwest::Error> {
        let headers = self.build_common_headers();

        // Return early
        if self.dry_run {
            let respdata: Vec<ResponseSearchData> = Vec::new();
            return Ok(ResponseNodes {
                count: 0,
                data: respdata,
            });
        }

        // Submit the POST request
        let response = self
            .client
            .get(url)
            .headers(headers)
            .query(&params)
            .send()
            .await?;

        // Check the HTTP status
        if response.status().is_success() {
            let body = response.json::<ResponseNodes>().await?;
            debug!("Found nodes: {:?}", body.data);
            Ok(body)
        } else {
            let status_code = response.status().as_u16();
            debug!("Request failed with status: {}", status_code);
            Self::check_auth_error(status_code);
            Err(response.error_for_status().err().unwrap())
        }
    }

    /// Upload a picture and return its ID
    /// If contents and md5sum are provided, they will be used instead of reading the file again
    pub async fn upload_picture(
        &mut self,
        pic_path: &Path,
        contents: Option<&[u8]>,
        md5sum: Option<&str>,
        conflict: ConflictResolution,
    ) -> Result<String, String> {
        if self.web_dir_node.is_none() && !self.dry_run {
            self.update_web_pictures_dir_node().await?;
        }

        // Read file contents if not provided
        let (contents_vec, md5sum_str, file_size) =
            if let (Some(data), Some(hash)) = (contents, md5sum) {
                // Use provided data and hash
                (data.to_vec(), hash.to_string(), data.len())
            } else {
                // Read file and compute hash
                let attr = fs::metadata(pic_path).map_err(|e| e.to_string())?;
                let mut contents_vec: Vec<u8> = Vec::with_capacity(attr.len() as usize);
                let mut file = File::open(pic_path).map_err(|e| e.to_string())?;
                let _ = file.read_to_end(&mut contents_vec);
                let computed_md5 = md5::compute(&contents_vec);
                (
                    contents_vec,
                    format!("{:x}", computed_md5),
                    attr.len() as usize,
                )
            };

        // Try to get capture date from EXIF data, or fall back to file creation time
        let capture_date_str = {
            let mut date_from_exif: Option<exif::DateTime> = None;

            // Try to read EXIF data
            let mut contents_cursor = std::io::Cursor::new(&contents_vec);
            let exif_reader = exif::Reader::new();
            if let Ok(exif_data) = exif_reader.read_from_container(&mut contents_cursor) {
                // Try DateTimeDigitized first (when the photo was taken)
                if let Some(field) = exif_data.get_field(Tag::DateTimeDigitized, exif::In::PRIMARY)
                {
                    if let Value::Ascii(ref vec) = field.value {
                        if !vec.is_empty() {
                            if let Ok(dt) = exif::DateTime::from_ascii(&vec[0]) {
                                date_from_exif = Some(dt);
                            }
                        }
                    }
                }

                // Fall back to DateTimeOriginal if DateTimeDigitized not found
                if date_from_exif.is_none() {
                    if let Some(field) =
                        exif_data.get_field(Tag::DateTimeOriginal, exif::In::PRIMARY)
                    {
                        if let Value::Ascii(ref vec) = field.value {
                            if !vec.is_empty() {
                                if let Ok(dt) = exif::DateTime::from_ascii(&vec[0]) {
                                    date_from_exif = Some(dt);
                                }
                            }
                        }
                    }
                }

                // Fall back to DateTime if neither DateTimeDigitized nor DateTimeOriginal found
                if date_from_exif.is_none() {
                    if let Some(field) = exif_data.get_field(Tag::DateTime, exif::In::PRIMARY) {
                        if let Value::Ascii(ref vec) = field.value {
                            if !vec.is_empty() {
                                if let Ok(dt) = exif::DateTime::from_ascii(&vec[0]) {
                                    date_from_exif = Some(dt);
                                }
                            }
                        }
                    }
                }
            }

            // If we have EXIF date, use it
            if let Some(exif_dt) = date_from_exif {
                format!(
                    "{:0>4}-{:0>2}-{:0>2}T{:0>2}:{:0>2}:{:0>2}.{:0>3}Z",
                    exif_dt.year,
                    exif_dt.month,
                    exif_dt.day,
                    exif_dt.hour,
                    exif_dt.minute,
                    exif_dt.second,
                    exif_dt.nanosecond.unwrap_or(0)
                )
            } else {
                // No EXIF data, check file creation/modification time
                let metadata = fs::metadata(pic_path).map_err(|e| e.to_string())?;

                // Get file creation or modification time
                let file_time = metadata
                    .created()
                    .or_else(|_| metadata.modified())
                    .map_err(|e| format!("Could not get file time: {}", e))?;

                // Check if file is more than 10 minutes old
                let now = SystemTime::now();
                let ten_minutes = std::time::Duration::from_secs(10 * 60);

                if let Ok(duration_since_file) = now.duration_since(file_time) {
                    if duration_since_file > ten_minutes {
                        // File is old enough, use its creation/modification time
                        let datetime: DateTime<Utc> = file_time.into();
                        datetime.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
                    } else {
                        // File is too recent, return error
                        return Err(format!(
                            "No EXIF data found and file is too recent (created/modified less than 10 minutes ago). \
                            Cannot determine capture date for: {}",
                            pic_path.display()
                        ));
                    }
                } else {
                    // File time is in the future somehow, use it anyway
                    let datetime: DateTime<Utc> = file_time.into();
                    datetime.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
                }
            }
        };

        let parent_node = match self.dry_run {
            false => &self.web_dir_node.as_ref().unwrap().id,
            true => "NONE",
        };

        // A file name is always the last path component; fall back to a lossy
        // rendering instead of panicking on non-UTF-8 names.
        let file_name = pic_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let url = Url::parse(&urls::upload_url(
            &self.conf.zone,
            conflict.wire_value(),
            file_size,
            &file_name,
            parent_node,
            &capture_date_str,
        ))
        .expect("Can't parse the upload URL");

        let mut headers = self.build_common_headers();
        headers.insert(
            "Content-Type",
            "application/x-www-form-urlencoded".parse().unwrap(),
        );
        headers.insert(
            "Accept",
            "application/json, text/plain, */*".parse().unwrap(),
        );
        headers.insert("x-amzn-file-md5", md5sum_str.parse().unwrap());

        // Return early
        if self.dry_run {
            return Ok("".to_string());
        }

        // Submit the POST request, retrying transient failures with backoff.
        // `contents_vec`/`headers` are reused for every attempt, hence the clones.
        let mut attempt: u32 = 0;
        let response = loop {
            attempt += 1;
            match self
                .client
                .post(url.clone())
                .headers(headers.clone())
                .body(contents_vec.clone())
                .send()
                .await
            {
                Ok(resp) => {
                    let status_code = resp.status().as_u16();
                    let transient_status =
                        status_code == 408 || status_code == 429 || resp.status().is_server_error();
                    if transient_status && attempt < UPLOAD_MAX_ATTEMPTS {
                        let delay = upload_retry_delay(attempt, Some(resp.headers()));
                        warn!(
                            "Upload attempt {}/{} for {} failed with HTTP {} - retrying in {:?}",
                            attempt,
                            UPLOAD_MAX_ATTEMPTS,
                            pic_path.display(),
                            status_code,
                            delay
                        );
                        tokio::time::sleep(delay).await;
                        continue;
                    }
                    if transient_status {
                        error!(
                            "Upload for {} failed with HTTP {} - giving up after {} attempts",
                            pic_path.display(),
                            status_code,
                            attempt
                        );
                    }
                    break resp;
                }
                Err(e) => {
                    let chain = error_chain(&e);
                    let transient = is_transient_reqwest_error(&e);
                    if transient && attempt < UPLOAD_MAX_ATTEMPTS {
                        let delay = upload_retry_delay(attempt, None);
                        warn!(
                            "Upload attempt {}/{} for {} failed ({}) - retrying in {:?}",
                            attempt,
                            UPLOAD_MAX_ATTEMPTS,
                            pic_path.display(),
                            chain,
                            delay
                        );
                        tokio::time::sleep(delay).await;
                        continue;
                    }
                    if transient {
                        error!(
                            "Upload for {} failed ({}) - giving up after {} attempts",
                            pic_path.display(),
                            chain,
                            attempt
                        );
                    }
                    return Err(chain);
                }
            }
        };

        // Check the HTTP status
        if response.status().is_success() {
            // Clone response to read body twice if needed
            let response_text = match response.text().await {
                Ok(t) => t,
                Err(e) => return Err(format!("Failed to read response body: {}", error_chain(&e))),
            };

            debug!(
                "Upload response (first 500 chars): {}",
                &response_text.chars().take(500).collect::<String>()
            );

            // Try to deserialize from the text
            let body: ResponseUpload = match serde_json::from_str(&response_text) {
                Ok(v) => v,
                Err(e) => {
                    error!("Failed to parse upload response as JSON: {}", e);
                    error!("Response body: {}", response_text);
                    return Err(format!("JSON parse error: {}", e));
                }
            };
            Ok(body.id)
        } else {
            let status_code = response.status().as_u16();
            debug!("Upload failed on status code {}", status_code);
            match status_code {
                // An upload conflict (HTTP 409). Who resolves it is the
                // caller's business: `upload_worker` treats it as "already
                // there" for `Ignore`, or asks the user per file for `Ask`
                // (docs/SPEC_DEDUP.md §4.1). The marker carries the server's
                // `errorCode` — so the worker can tell a content duplicate
                // (`DuplicatesConflictError`) from a name conflict
                // (`NameAlreadyExists`) — plus the existing node id.
                409 => {
                    let body = match response.json::<ResponseUploadFail>().await {
                        Ok(v) => v,
                        Err(e) => return Err(error_chain(&e)),
                    };
                    let existing = body
                        .error_details
                        .conflict_node_ids
                        .first()
                        .cloned()
                        .unwrap_or_default();
                    return Err(conflict_error(&body.error_code, &existing));
                }
                _ => {
                    // Check for authentication errors
                    if Self::check_auth_error(status_code) {
                        return Err("Authentication failed: Session cookies expired".to_string());
                    }
                    let body = match response.text().await {
                        Ok(v) => v,
                        Err(e) => {
                            return Err(format!(
                                "Failed to read error response body: {}",
                                error_chain(&e)
                            ));
                        }
                    };
                    let snippet: String = body.chars().take(300).collect();
                    error!(
                        "Upload failed for {} with HTTP {}: {}",
                        pic_path.display(),
                        status_code,
                        snippet
                    );
                    Err(format!("HTTP {}: {}", status_code, snippet))
                }
            }
        }
    }

    pub async fn add_to_album(
        &self,
        album_id: String,
        uploaded_ids: &[String],
    ) -> Result<(), reqwest::Error> {
        let url = Url::parse(&urls::add_to_album_url(&self.conf.country, &album_id))
            .expect("Can't parse the add to album URL");

        let mut headers = self.build_common_headers();
        // Override Content-Type for this specific request
        headers.insert(
            "Content-Type",
            "application/x-www-form-urlencoded".parse().unwrap(),
        );
        headers.insert(
            "Accept",
            "application/json, text/plain, */*".parse().unwrap(),
        );

        let payload = QueryNodeModify {
            op: "add".to_string(),
            value: uploaded_ids.to_vec(),
            resource_version: "V2".to_string(),
            content_type: "JSON".to_string(),
        };

        // Return early
        if self.dry_run {
            return Ok(());
        }

        // Submit the PATCH request
        let response = self
            .client
            .patch(url)
            .headers(headers)
            .json(&payload)
            .send()
            .await?;

        // Check the HTTP status
        if response.status().is_success() {
            Ok(())
        } else {
            let status_code = response.status().as_u16();
            debug!("Request failed with status: {}", status_code);
            Self::check_auth_error(status_code);
            Err(response.error_for_status().err().unwrap())
        }
    }

    /// Fetch the signed-in customer's display name and avatar node id from the
    /// account preferences endpoint (`docs/SPEC_ACCOUNT_BILLING.md` §3.4).
    ///
    /// `Ok(None)` when the response carries no usable name **or** no avatar
    /// node id (missing, empty, whitespace-only) — never panics, and a missing
    /// field does not fail the parse. Uses the same cookie headers as the other
    /// drive calls (`build_common_headers`).
    pub async fn fetch_account_name(&self) -> Result<Option<(String, String)>, reqwest::Error> {
        let url = Url::parse(&urls::preferences_url(&self.conf.country))
            .expect("Can't parse the preferences URL");
        let headers = self.build_common_headers();

        if self.dry_run {
            return Ok(None);
        }

        let response = self.client.get(url).headers(headers).send().await?;

        if response.status().is_success() {
            let body = response.json::<ResponsePreferences>().await?;
            let name = body.name.trim().to_string();
            let avatar = body.avatar_node_id.unwrap_or_default().trim().to_string();
            if name.is_empty() || avatar.is_empty() {
                Ok(None)
            } else {
                Ok(Some((name, avatar)))
            }
        } else {
            let status_code = response.status().as_u16();
            debug!("Preferences request failed with status: {}", status_code);
            Self::check_auth_error(status_code);
            Err(response.error_for_status().err().unwrap())
        }
    }

    /// Download the 60×60 avatar thumbnail as **direct JPEG bytes** from the
    /// thumbnail service (`docs/SPEC_ACCOUNT_BILLING.md` §3.4). The
    /// device-specific `x-amz-access-token` the original sends is deliberately
    /// **not** used — our cookie headers are sufficient here and the token
    /// would make the request fail with 401.
    pub async fn fetch_avatar_jpeg(&self, avatar_node_id: &str) -> Result<Vec<u8>, reqwest::Error> {
        let url = Url::parse(&urls::thumbnail_url(avatar_node_id, &self.conf.country))
            .expect("Can't parse the thumbnail URL");
        let headers = self.build_common_headers();

        if self.dry_run {
            return Ok(Vec::new());
        }

        let response = self.client.get(url).headers(headers).send().await?;

        if response.status().is_success() {
            Ok(response.bytes().await?.to_vec())
        } else {
            let status_code = response.status().as_u16();
            debug!("Thumbnail request failed with status: {}", status_code);
            Self::check_auth_error(status_code);
            Err(response.error_for_status().err().unwrap())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn avatar_cache_file_name_is_node_and_viewbox() {
        assert_eq!(avatar_cache_file_name("NODE123"), "NODE123_60_60.jpeg");
    }

    #[test]
    fn avatar_cache_path_lives_under_thumbnails() {
        let p = avatar_cache_path_in(Path::new("/base/prime_photos"), "NODE123");
        assert_eq!(
            p,
            Path::new("/base/prime_photos/thumbnails/NODE123_60_60.jpeg")
        );
    }

    #[test]
    fn looks_like_image_accepts_jpeg_and_png_rejects_junk() {
        // JPEG SOI + marker.
        assert!(looks_like_image(&[0xFF, 0xD8, 0xFF, 0xE0, 0x00]));
        // PNG signature.
        assert!(looks_like_image(&[
            0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00
        ]));
        assert!(!looks_like_image(b"not an image"));
        assert!(!looks_like_image(&[]));
    }

    #[test]
    fn conflict_marker_round_trips_code_and_node_id() {
        let err = conflict_error("DuplicatesConflictError", "NODE123");
        assert_eq!(conflict_node_id(&err), Some("NODE123"));
        assert_eq!(conflict_cause(&err), Some("DuplicatesConflictError"));
    }

    #[test]
    fn conflict_marker_keeps_an_empty_node_id() {
        // The server sent `conflictNodeIds` with no usable entry.
        let err = conflict_error("NameAlreadyExists", "");
        assert_eq!(conflict_node_id(&err), Some(""));
        assert_eq!(conflict_cause(&err), Some("NameAlreadyExists"));
    }

    #[test]
    fn a_plain_error_has_no_conflict_code_or_node_id() {
        assert_eq!(conflict_node_id("plain: connection reset"), None);
        assert_eq!(conflict_cause("plain: connection reset"), None);
    }

    #[test]
    fn content_duplicate_codes_are_recognised_case_insensitively() {
        assert!(is_content_duplicate_code("DuplicatesConflictError"));
        assert!(is_content_duplicate_code("duplicatesconflicterror"));
        assert!(is_content_duplicate_code("DUPLICATESCONFLICTERROR"));
        assert!(is_content_duplicate_code("MD5_DUPLICATE"));
        assert!(is_content_duplicate_code("md5_duplicate"));
    }

    #[test]
    fn name_conflicts_unknown_and_empty_codes_are_not_content_duplicates() {
        assert!(!is_content_duplicate_code("NameAlreadyExists"));
        assert!(!is_content_duplicate_code("NAME_ALREADY_EXISTS"));
        assert!(!is_content_duplicate_code(""));
        assert!(!is_content_duplicate_code("SomethingElse"));
    }
}
