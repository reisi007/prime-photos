//! The **single** place where marketplace- and zone-dependent URLs are built
//! (project rule, see `AGENTS.md`).
//!
//! `config.country` is a **code** (`de`, `uk`, `jp`, `au`, `br`, `mx`, …) as
//! read from the `x-acb<code>` cookie name by `countries::parse_cookie_string`,
//! **not** the marketplace domain. Every Amazon host therefore has to be
//! derived through [`marketplace_domain`] instead of interpolating the code —
//! for `uk`/`jp`/`au`/`br`/`mx` the two differ, and the raw code would hit a
//! host that does not exist.
//!
//! No `https://www.amazon.`, `thumbnails-photos.amazon.` or
//! `content-<zone>.drive.amazonaws.com` literal may appear anywhere else in the
//! crate, and the helper tests live here too.

use crate::prime_photos::AVATAR_VIEW_BOX;

/// Amazon marketplace domain ("suffix") for a marketplace **code**.
///
/// Codes that already are their domain (`de`, `fr`, `it`, `es`, `ca`, `cn`,
/// `in`) pass through unchanged; the aliases `uk`/`gb` and `at`/`ch` map to
/// their real marketplace, as does every multi-label suffix.
pub(crate) fn marketplace_domain(country: &str) -> String {
    match country {
        "us" => "com".to_string(),
        "uk" | "gb" => "co.uk".to_string(),
        "at" | "ch" => "de".to_string(),
        "jp" => "co.jp".to_string(),
        "mx" => "com.mx".to_string(),
        "br" => "com.br".to_string(),
        "au" => "com.au".to_string(),
        "cn" => "cn".to_string(),
        "in" => "in".to_string(),
        other => other.to_string(),
    }
}

/// Amazon's Photos sign-in URL (two-column layout). The marketplace is encoded
/// in the domain and in the assoc handle (`amzn_photos_web_<cc>`); `at`/`ch`
/// sign in on the German marketplace.
pub(crate) fn login_url(country: &str) -> String {
    let domain = marketplace_domain(country);
    let cc = match country {
        "at" | "ch" => "de",
        other => other,
    };
    let return_to = format!(
        "https%3A%2F%2Fwww.amazon.{d}%2Fphotos%3Fsf%3D1%26ref_%3DAPH_D_PA1_{cc}",
        d = domain,
        cc = cc.to_uppercase()
    );
    format!(
        "https://www.amazon.{domain}/ap/signin?openid.pape.max_auth_age=3600\
&openid.return_to={return_to}\
&openid.identity=http%3A%2F%2Fspecs.openid.net%2Fauth%2F2.0%2Fidentifier_select\
&openid.assoc_handle=amzn_photos_web_{cc}\
&openid.mode=checkid_setup\
&openid.claimed_id=http%3A%2F%2Fspecs.openid.net%2Fauth%2F2.0%2Fidentifier_select\
&openid.ns=http%3A%2F%2Fspecs.openid.net%2Fauth%2F2.0"
    )
}

/// `photos/auth/token` endpoint (auth-token renewal for content uploads).
pub(crate) fn auth_token_url(country: &str) -> String {
    format!(
        "https://www.amazon.{}/photos/auth/token",
        marketplace_domain(country)
    )
}

// ---------------------------------------------------------------------------
// Device login flow (user decision 2026-10-15, `docs/SPEC_LOGIN.md` § 3.8).
//
// These helpers are the **pure** URL builders of the device/PKCE sign-in path
// and are deliberately not wired into the app yet (wiring follows in a later
// package), so each carries `#[allow(dead_code)]` — the `AGENTS.md`
// "prepared hook" exception, to be removed once the flow is wired. They are
// *not* the web/cookie path: [`login_url`] and [`auth_token_url`] above stay
// untouched.
// ---------------------------------------------------------------------------

/// Marketplace identity of the **device** login flow: the Amazon host base, the
/// numeric `marketPlaceId` and the lowercase region code that the
/// `amzn_photos_desktop_<region>` assoc handle and `pageId` carry.
///
/// Mirrors the original's fixed marketplace table (recon 2026-10-15): the
/// aliases `gb`/`uk` resolve to the British marketplace, `at`/`ch` sign in on
/// the German one, and **everything unknown falls back to US** — including
/// `mx` and `in`, which *are* selectable marketplaces but are not in the
/// device table, exactly like the original.
#[allow(dead_code)]
pub(crate) struct AuthMarketplace {
    /// Host base (`amazon.de`, `amazon.co.uk`, …).
    pub(crate) domain: String,
    /// Numeric `marketPlaceId` sent in the portal URL and, later, the requests.
    pub(crate) marketplace_id: String,
    /// Lowercase region for `amzn_photos_desktop_<region>` (`gb` → `uk`).
    pub(crate) region: String,
}

/// Resolve a marketplace **code** to its [`AuthMarketplace`] (table above).
#[allow(dead_code)]
pub(crate) fn auth_marketplace(country: &str) -> AuthMarketplace {
    let (id, domain, region) = match country {
        "au" => ("A39IBJ37TRP1C6", "com.au", "au"),
        "br" => ("A2Q3Y263D00KWC", "com.br", "br"),
        "ca" => ("A2EUQ1WTGCTBG2", "ca", "ca"),
        "cn" => ("AAHKV2X7AFYLW", "cn", "cn"),
        "de" | "at" | "ch" => ("A1PA6795UKMFR9", "de", "de"),
        "es" => ("A1RKKUPIHCS9HS", "es", "es"),
        "fr" => ("A13V1IB3VIYZZH", "fr", "fr"),
        "gb" | "uk" => ("A1F83G8C2ARO7P", "co.uk", "uk"),
        "it" => ("APJ6JRA9NG5V4", "it", "it"),
        "jp" => ("A1VC38T7YXB528", "co.jp", "jp"),
        // Unknown codes (incl. `mx`, `in`) fall back to US, like the original.
        _ => ("ATVPDKIKX0DER", "com", "us"),
    };
    AuthMarketplace {
        domain: format!("amazon.{domain}"),
        marketplace_id: id.to_string(),
        region: region.to_string(),
    }
}

/// Amazon's **device** sign-in URL (PKCE). The parameters are sent in the
/// original's exact order and **raw** — the original concatenates the query
/// without percent-encoding, so the same values are passed through verbatim
/// here (this is the deviation that makes the flow recognisable to Amazon).
#[allow(dead_code)]
pub(crate) fn auth_portal_url(
    country: &str,
    client_id: &str,
    code_challenge: &str,
    language: &str,
) -> String {
    let mp = auth_marketplace(country);
    let return_to = format!("https://www.{}/ap/maplanding", mp.domain);
    let handle = format!("amzn_photos_desktop_{}", mp.region);
    format!(
        "https://www.{domain}/ap/signin?\
openid.ns=http://specs.openid.net/auth/2.0\
&openid.claimed_id=http://specs.openid.net/auth/2.0/identifier_select\
&openid.identity=http://specs.openid.net/auth/2.0/identifier_select\
&openid.mode=checkid_setup\
&openid.oa2.scope=device_auth_access\
&openid.ns.oa2=http://www.amazon.com/ap/ext/oauth/2\
&openid.oa2.response_type=code\
&openid.oa2.code_challenge_method=S256\
&openid.oa2.client_id={client_id}\
&language={language}\
&marketPlaceId={mp_id}\
&openid.return_to={return_to}\
&openid.pape.max_auth_age=0\
&forceMobileLayout=true\
&openid.assoc_handle={handle}\
&pageId={handle}\
&openid.oa2.code_challenge={code_challenge}",
        domain = mp.domain,
        mp_id = mp.marketplace_id,
    )
}

/// Panda (`api.amazon.<tld>`) host base for a marketplace. The host is the
/// marketplace domain with `www` replaced by `api` (DE → `api.amazon.de`,
/// JP → `api.amazon.co.jp`).
#[allow(dead_code)]
pub(crate) fn panda_base_url(country: &str) -> String {
    format!("https://api.{}", auth_marketplace(country).domain)
}

/// `POST <base>/auth/register` (exchange the PKCE authorization code).
#[allow(dead_code)]
pub(crate) fn panda_register_url(country: &str) -> String {
    format!("{}/auth/register", panda_base_url(country))
}

/// `POST <base>/auth/token` (refresh an access token, or request auth cookies).
#[allow(dead_code)]
pub(crate) fn panda_token_url(country: &str) -> String {
    format!("{}/auth/token", panda_base_url(country))
}

/// `POST <base>/auth/deregister` (sign out).
#[allow(dead_code)]
pub(crate) fn panda_deregister_url(country: &str) -> String {
    format!("{}/auth/deregister", panda_base_url(country))
}

/// `POST <base>/auth/token` with the **auth-cookies** query (device login
/// step 6): the refresh token is the source, and the answer is a
/// domain → cookie-list map that feeds the existing cookie upload chain.
///
/// The query order follows the original; only `app_name`/`app_version` are
/// percent-encoded (the refresh token and `domain` are passed through raw).
#[allow(dead_code)]
pub(crate) fn panda_auth_cookies_url(
    country: &str,
    refresh_token: &str,
    app_name: &str,
    app_version: &str,
) -> String {
    let mp = auth_marketplace(country);
    format!(
        "{base}/auth/token?source_token={refresh_token}\
&source_token_type=refresh_token\
&requested_token_type=auth_cookies\
&domain=.{domain}\
&app_name={app_name}\
&app_version={app_version}",
        base = panda_base_url(country),
        domain = mp.domain,
        app_name = percent_encode(app_name),
        app_version = percent_encode(app_version),
    )
}

/// Minimal percent-encoder for the auth-cookies query. The unreserved set
/// `A–Z a–z 0–9 - _ . ~` is kept verbatim (RFC 3986); everything else becomes
/// `%XX` uppercase. A full encoder is not needed — only two known strings
/// (app name/version) pass through here.
#[allow(dead_code)]
fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// `POST drive/v1/nodes` (create an album / node).
pub(crate) fn nodes_url(country: &str) -> String {
    format!(
        "https://www.amazon.{}/drive/v1/nodes",
        marketplace_domain(country)
    )
}

/// `drive/v1/nodes/` base (node lookup via `.query(...)`).
pub(crate) fn nodes_base_url(country: &str) -> String {
    format!(
        "https://www.amazon.{}/drive/v1/nodes/",
        marketplace_domain(country)
    )
}

/// `drive/v1/search` with the caller's already-built query string appended.
pub(crate) fn search_url(country: &str, query: &str) -> String {
    format!(
        "https://www.amazon.{}/drive/v1/search{}",
        marketplace_domain(country),
        query
    )
}

/// `drive/v1/nodes/{id}/children` **with** the full inline query string used by
/// the folder walk (`asset`/`filters`/…/`_`).
pub(crate) fn children_nodes_url(
    country: &str,
    node_id: &str,
    filter: &str,
    epoch_millis: &str,
) -> String {
    format!(
        "https://www.amazon.{}/drive/v1/nodes/{}/children?asset=ALL&filters={}&searchOnFamily=false&tempLink=false&offset=0&resourceVersion=V2&ContentType=JSON&_={}",
        marketplace_domain(country),
        node_id,
        filter,
        epoch_millis
    )
}

/// `drive/v1/nodes/{id}/children` (bare path, used to add uploads to an album).
pub(crate) fn add_to_album_url(country: &str, album_id: &str) -> String {
    format!(
        "https://www.amazon.{}/drive/v1/nodes/{}/children",
        marketplace_domain(country),
        album_id
    )
}

/// `GET drive/v1/account/preferences` (`name` + `avatarNodeId`).
pub(crate) fn preferences_url(country: &str) -> String {
    format!(
        "https://www.amazon.{}/drive/v1/account/preferences",
        marketplace_domain(country)
    )
}

/// `GET v1/thumbnail/<id>` on the **marketplace's** thumbnail host
/// (`thumbnails-photos.amazon.<domain>`, `docs/PROTOCOL.md` §2.4). `viewBox` is
/// percent-encoded (`60%2C60`), exactly as the original sends it.
pub(crate) fn thumbnail_url(node_id: &str, country: &str) -> String {
    let domain = marketplace_domain(country);
    format!(
        "https://thumbnails-photos.amazon.{domain}/v1/thumbnail/{node_id}?viewBox={v}%2C{v}&fit=clip&encoding=jpeg",
        v = AVATAR_VIEW_BOX
    )
}

/// `POST content-<zone>.drive.amazonaws.com/v2/upload` with the query in the
/// order the desktop client uses (`conflictResolution`, `fileSize`, `name`,
/// `parentNodeId`, `contentDate`). `name` is normalised to Unicode **NFC**
/// before it enters the query ([`crate::text::nfc`]): macOS hands out NFD names
/// and the server compares names exactly, so an unnormalised NFD name would not
/// match a node created from Windows (see `text::nfc`). Nothing is encoded — the
/// original does not pre-encode the name either.
///
/// The query itself is built by [`upload_query`] and shared with the replace
/// `PUT` ([`replace_upload_url`]), so the two stages cannot drift apart.
pub(crate) fn upload_url(
    zone: &str,
    conflict_resolution: &str,
    file_size: usize,
    name: &str,
    parent_node_id: &str,
    content_date: &str,
) -> String {
    format!(
        "https://content-{zone}.drive.amazonaws.com/v2/upload?{q}",
        q = upload_query(
            conflict_resolution,
            file_size,
            name,
            parent_node_id,
            content_date
        )
    )
}

/// The query string **both** upload stages carry (`conflictResolution`,
/// `fileSize`, `name`, `parentNodeId`, `contentDate`), in the order the desktop
/// client uses. Built once here so the `POST` ([`upload_url`]) and the replace
/// `PUT` ([`replace_upload_url`]) can never disagree: live-proven 2026-10-15,
/// the host answers a `PUT` without `fileSize` with HTTP 400
/// `MissingFileSize` — the replace stage must therefore send exactly the POST's
/// set, only the path differs (the node id moves into it).
///
/// `name` is normalised to Unicode **NFC** ([`crate::text::nfc`]) — see
/// [`upload_url`] for the full reasoning. Nothing is percent-encoded, matching
/// the original.
fn upload_query(
    conflict_resolution: &str,
    file_size: usize,
    name: &str,
    parent_node_id: &str,
    content_date: &str,
) -> String {
    let name = crate::text::nfc(name);
    format!(
        "conflictResolution={}&fileSize={}&name={}&parentNodeId={}&contentDate={}",
        conflict_resolution, file_size, name, parent_node_id, content_date
    )
}

/// `PUT content-<zone>.drive.amazonaws.com/v2/upload/<nodeId>` — the second
/// stage of `ConflictResolution::Replace` (user decision 2026-10-15). The POST
/// asks for `conflictResolution=NONE` (the host rejects `OVERWRITE` with 404 and
/// an empty body), so a taken name comes back as HTTP 409 naming the node that
/// is already there; that node is then replaced through this URL — the desktop
/// client's own overwrite path (`overwrite=true` + `nodeId`), live-proven: 200,
/// the **same** node id, content actually swapped.
///
/// The `PUT` carries **the same query set as the POST** ([`upload_query`]:
/// `conflictResolution=NONE`, `fileSize`, `name` (NFC-normalised),
/// `parentNodeId`, `contentDate`; only the path differs (the node id takes the
/// place of the bare `/v2/upload`). Live-proven 2026-10-15: without `fileSize`
/// the host rejects the `PUT` with HTTP **400 `MissingFileSize`** ("File size
/// should not be empty."). Body (the file bytes) and headers are the POST's,
/// too ([`crate::prime_photos::PrimePhotos::replace_picture`]).
///
/// The parameters mirror [`upload_url`]'s, plus the `node_id`.
///
/// The node id is **not** NFC-normalised (unlike [`upload_url`]'s `name`),
/// because it is a **server** value — an opaque id echoed back in
/// `errorDetails.conflictNodeIds[0]`, never a file name a filesystem could
/// spell differently per OS. There is no NFD/NFC variant of an id that macOS and
/// Windows could disagree about, so normalising could only corrupt it.
pub(crate) fn replace_upload_url(
    zone: &str,
    node_id: &str,
    conflict_resolution: &str,
    file_size: usize,
    name: &str,
    parent_node_id: &str,
    content_date: &str,
) -> String {
    format!(
        "https://content-{zone}.drive.amazonaws.com/v2/upload/{node_id}?{q}",
        q = upload_query(
            conflict_resolution,
            file_size,
            name,
            parent_node_id,
            content_date
        )
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marketplace_domain_maps_every_alias_to_its_real_host() {
        assert_eq!(marketplace_domain("us"), "com");
        assert_eq!(marketplace_domain("uk"), "co.uk");
        assert_eq!(marketplace_domain("gb"), "co.uk");
        assert_eq!(marketplace_domain("jp"), "co.jp");
        assert_eq!(marketplace_domain("au"), "com.au");
        assert_eq!(marketplace_domain("br"), "com.br");
        assert_eq!(marketplace_domain("mx"), "com.mx");
        assert_eq!(marketplace_domain("at"), "de");
        assert_eq!(marketplace_domain("ch"), "de");
        assert_eq!(marketplace_domain("cn"), "cn");
        assert_eq!(marketplace_domain("in"), "in");
    }

    #[test]
    fn marketplace_domain_passes_through_code_equal_to_domain() {
        // Codes that already are their domain must stay byte-identical.
        assert_eq!(marketplace_domain("de"), "de");
        assert_eq!(marketplace_domain("fr"), "fr");
        assert_eq!(marketplace_domain("it"), "it");
        assert_eq!(marketplace_domain("es"), "es");
        assert_eq!(marketplace_domain("ca"), "ca");
    }

    #[test]
    fn login_url_uses_marketplace_domain() {
        assert!(login_url("de").starts_with("https://www.amazon.de/ap/signin?"));
        // The alias resolves to the real host, not `amazon.uk`.
        assert!(login_url("uk").starts_with("https://www.amazon.co.uk/ap/signin?"));
        // `at`/`ch` sign in on the German marketplace.
        assert!(login_url("at").starts_with("https://www.amazon.de/ap/signin?"));
        assert!(login_url("jp").contains("amazon.co.jp"));
    }

    #[test]
    fn auth_token_url_uses_marketplace_domain() {
        assert_eq!(
            auth_token_url("de"),
            "https://www.amazon.de/photos/auth/token"
        );
        assert_eq!(
            auth_token_url("uk"),
            "https://www.amazon.co.uk/photos/auth/token"
        );
    }

    #[test]
    fn nodes_urls_use_marketplace_domain() {
        assert_eq!(nodes_url("de"), "https://www.amazon.de/drive/v1/nodes");
        assert_eq!(nodes_url("jp"), "https://www.amazon.co.jp/drive/v1/nodes");
        assert_eq!(
            nodes_base_url("de"),
            "https://www.amazon.de/drive/v1/nodes/"
        );
        assert_eq!(
            nodes_base_url("mx"),
            "https://www.amazon.com.mx/drive/v1/nodes/"
        );
    }

    #[test]
    fn search_url_appends_the_query_verbatim() {
        assert_eq!(
            search_url("de", "?asset=ALL&limit=200"),
            "https://www.amazon.de/drive/v1/search?asset=ALL&limit=200"
        );
        assert_eq!(
            search_url("br", "?x=1"),
            "https://www.amazon.com.br/drive/v1/search?x=1"
        );
    }

    #[test]
    fn children_nodes_url_keeps_the_inline_query_order() {
        assert_eq!(
            children_nodes_url("de", "NODE1", "isRoot:true", "123"),
            "https://www.amazon.de/drive/v1/nodes/NODE1/children?asset=ALL&filters=isRoot:true&searchOnFamily=false&tempLink=false&offset=0&resourceVersion=V2&ContentType=JSON&_=123"
        );
        assert_eq!(
            children_nodes_url("au", "N", "f", "9"),
            "https://www.amazon.com.au/drive/v1/nodes/N/children?asset=ALL&filters=f&searchOnFamily=false&tempLink=false&offset=0&resourceVersion=V2&ContentType=JSON&_=9"
        );
    }

    #[test]
    fn add_to_album_url_uses_marketplace_domain() {
        assert_eq!(
            add_to_album_url("de", "ALB1"),
            "https://www.amazon.de/drive/v1/nodes/ALB1/children"
        );
        assert_eq!(
            add_to_album_url("uk", "ALB1"),
            "https://www.amazon.co.uk/drive/v1/nodes/ALB1/children"
        );
    }

    #[test]
    fn preferences_url_uses_marketplace_country() {
        assert_eq!(
            preferences_url("de"),
            "https://www.amazon.de/drive/v1/account/preferences"
        );
        assert_eq!(
            preferences_url("fr"),
            "https://www.amazon.fr/drive/v1/account/preferences"
        );
        // The alias is corrected (raw code would have built `amazon.uk`).
        assert_eq!(
            preferences_url("uk"),
            "https://www.amazon.co.uk/drive/v1/account/preferences"
        );
    }

    #[test]
    fn thumbnail_url_encodes_viewbox_and_node() {
        assert_eq!(
            thumbnail_url("B0ABCDEF123", "de"),
            "https://thumbnails-photos.amazon.de/v1/thumbnail/B0ABCDEF123\
             ?viewBox=60%2C60&fit=clip&encoding=jpeg"
        );
        // The comma must be percent-encoded, exactly as the original sends it.
        let url = thumbnail_url("NODE", "de");
        assert!(url.contains("viewBox=60%2C60"));
        assert!(!url.contains("viewBox=60,60"));
    }

    #[test]
    fn thumbnail_url_uses_the_marketplace_host() {
        // The host follows the shared `marketplace_domain` mapping, so a
        // non-German marketplace no longer hits `thumbnails-photos.amazon.de`.
        assert!(thumbnail_url("NODE", "us").starts_with("https://thumbnails-photos.amazon.com/"));
        assert!(thumbnail_url("NODE", "uk").starts_with("https://thumbnails-photos.amazon.co.uk/"));
        assert!(thumbnail_url("NODE", "fr").starts_with("https://thumbnails-photos.amazon.fr/"));
        // "at"/"ch" map to the German marketplace, hence the `.de` host.
        assert!(thumbnail_url("NODE", "at").starts_with("https://thumbnails-photos.amazon.de/"));
        // The path/query is unchanged by the marketplace.
        assert!(
            thumbnail_url("NODE", "us")
                .ends_with("/v1/thumbnail/NODE?viewBox=60%2C60&fit=clip&encoding=jpeg")
        );
    }

    #[test]
    fn replace_upload_url_is_zone_and_node_exact() {
        // The German marketplace uploads into the `eu` zone.
        assert_eq!(
            replace_upload_url(
                "eu",
                "NODE1",
                "NONE",
                1234,
                "foto.jpg",
                "PARENT",
                "2020-01-02T03:04:05.000Z"
            ),
            "https://content-eu.drive.amazonaws.com/v2/upload/NODE1?conflictResolution=NONE&fileSize=1234&name=foto.jpg&parentNodeId=PARENT&contentDate=2020-01-02T03:04:05.000Z"
        );
        // A non-German marketplace (e.g. `us` → the `na` zone) uses its own
        // zone; only the host changes, path and query stay the same shape.
        assert_eq!(
            replace_upload_url(
                "na",
                "B0ABCDEF123",
                "NONE",
                7,
                "a b.jpg",
                "P",
                "2020-01-02T03:04:05.000Z"
            ),
            "https://content-na.drive.amazonaws.com/v2/upload/B0ABCDEF123?conflictResolution=NONE&fileSize=7&name=a b.jpg&parentNodeId=P&contentDate=2020-01-02T03:04:05.000Z"
        );
        // The replace stage never sends the `OVERWRITE` value the host rejects.
        let url = replace_upload_url("eu", "NODE1", "NONE", 1, "f.jpg", "P", "d");
        assert!(!url.contains("OVERWRITE"));
        // The node id is a server value and is passed through byte-exactly.
        assert!(url.contains("/v2/upload/NODE1?"));
    }

    #[test]
    fn replace_upload_url_carries_the_same_query_as_the_post() {
        // The invariant behind the 2026-10-15 live fix: the host rejects a
        // `PUT` without `fileSize` with HTTP 400 `MissingFileSize`, so the
        // replace stage must send exactly the POST's query set — the only
        // difference between the two URLs is the node id in the path.
        let post = upload_url(
            "eu",
            "NONE",
            1234,
            "foto.jpg",
            "PARENT",
            "2020-01-02T03:04:05.000Z",
        );
        let put = replace_upload_url(
            "eu",
            "NODE1",
            "NONE",
            1234,
            "foto.jpg",
            "PARENT",
            "2020-01-02T03:04:05.000Z",
        );
        let post_query = post.split_once('?').expect("POST URL has a query").1;
        let put_query = put.split_once('?').expect("PUT URL has a query").1;
        assert_eq!(post_query, put_query);
        // …and it is the full set, not just `conflictResolution`.
        assert!(put.contains("conflictResolution=NONE"));
        assert!(put.contains("fileSize=1234"));
        assert!(put.contains("name=foto.jpg"));
        assert!(put.contains("parentNodeId=PARENT"));
        assert!(put.contains("contentDate=2020-01-02T03:04:05.000Z"));
        // The PUT must be passed the **same** name, otherwise the two URLs
        // would normalise differently: `name` is NFC-folded in both stages.
        let nfd = "O\u{308}FB-Reisinger_01.dng";
        let put = replace_upload_url("eu", "NODE1", "NONE", 7, nfd, "PARENT", "d");
        assert!(put.contains("name=\u{D6}FB-Reisinger_01.dng"));
        assert!(!put.contains('\u{308}'));
    }

    #[test]
    fn upload_url_is_zone_and_query_exact() {
        assert_eq!(
            upload_url(
                "eu",
                "RENAME",
                1234,
                "foto.jpg",
                "NODE1",
                "2020-01-02T03:04:05.000Z"
            ),
            "https://content-eu.drive.amazonaws.com/v2/upload?conflictResolution=RENAME&fileSize=1234&name=foto.jpg&parentNodeId=NODE1&contentDate=2020-01-02T03:04:05.000Z"
        );
        // A different zone changes only the host; the query order is fixed.
        assert_eq!(
            upload_url("na", "OVERWRITE", 5, "a b.jpg", "P", "d"),
            "https://content-na.drive.amazonaws.com/v2/upload?conflictResolution=OVERWRITE&fileSize=5&name=a b.jpg&parentNodeId=P&contentDate=d"
        );
    }

    #[test]
    fn upload_url_normalises_nfd_names_to_nfc() {
        // macOS hands out NFD ("O" + combining diaeresis); the server compares
        // names exactly, so the request must carry the single NFC codepoint or a
        // name written from Windows would not match.
        let nfd = "O\u{308}FB-Reisinger_01.dng";
        let url = upload_url("eu", "OVERWRITE", 7, nfd, "NODE", "2026-09-28T00:00:00.000Z");
        assert!(url.contains("name=\u{D6}FB-Reisinger_01.dng"));
        assert!(!url.contains('\u{308}'));
        // An already-NFC name is passed through unchanged.
        let url = upload_url("eu", "NONE", 7, "\u{D6}FB.dng", "NODE", "d");
        assert!(url.contains("name=\u{D6}FB.dng"));
    }

    /// The node id is a **server** value (an echo of the 409's
    /// `errorDetails.conflictNodeIds[0]`), so [`replace_upload_url`] passes it
    /// through **byte-identically** — unlike `name`, which is folded to NFC. An
    /// NFD id must survive exactly as it came in: normalising a server id would
    /// ask for a node that does not exist under the folded spelling.
    #[test]
    fn replace_upload_url_passes_a_non_ascii_node_id_through_byte_exactly() {
        // "o" + combining diaeresis (NFD) — the two codepoints a macOS-spelled
        // value carries; NFC folds them into the single "ö".
        let nfd = "B0o\u{308}x\u{F6}";
        assert_ne!(nfd, crate::text::nfc(nfd));
        let url = replace_upload_url("eu", nfd, "NONE", 7, "O\u{308}FB.jpg", "P", "d");
        assert_eq!(
            url,
            "https://content-eu.drive.amazonaws.com/v2/upload/B0o\u{308}x\u{F6}?conflictResolution=NONE&fileSize=7&name=\u{D6}FB.jpg&parentNodeId=P&contentDate=d"
        );
        // …so within one and the same URL the two treatments are visible: the
        // **raw** NFD id in the path, the **folded** name in the query.
        assert!(url.contains("/v2/upload/B0o\u{308}x\u{F6}?"));
        // Nothing of the id was folded away — the NFC form is nowhere in the URL.
        assert!(!url.contains(&crate::text::nfc(nfd)));
        // The name in the same URL is the folded one, so the assertion above is
        // not vacuous.
        assert!(url.contains("name=\u{D6}FB.jpg"));
    }

    #[test]
    fn auth_marketplace_follows_the_device_table() {
        // The full table of the device flow.
        for (code, id, domain, region) in [
            ("au", "A39IBJ37TRP1C6", "amazon.com.au", "au"),
            ("br", "A2Q3Y263D00KWC", "amazon.com.br", "br"),
            ("ca", "A2EUQ1WTGCTBG2", "amazon.ca", "ca"),
            ("cn", "AAHKV2X7AFYLW", "amazon.cn", "cn"),
            ("de", "A1PA6795UKMFR9", "amazon.de", "de"),
            ("es", "A1RKKUPIHCS9HS", "amazon.es", "es"),
            ("fr", "A13V1IB3VIYZZH", "amazon.fr", "fr"),
            ("gb", "A1F83G8C2ARO7P", "amazon.co.uk", "uk"),
            ("uk", "A1F83G8C2ARO7P", "amazon.co.uk", "uk"),
            ("it", "APJ6JRA9NG5V4", "amazon.it", "it"),
            ("jp", "A1VC38T7YXB528", "amazon.co.jp", "jp"),
            ("us", "ATVPDKIKX0DER", "amazon.com", "us"),
        ] {
            let mp = auth_marketplace(code);
            assert_eq!(mp.marketplace_id, id, "marketPlaceId for {code}");
            assert_eq!(mp.domain, domain, "domain for {code}");
            assert_eq!(mp.region, region, "region for {code}");
        }
    }

    #[test]
    fn auth_marketplace_maps_at_and_ch_to_de() {
        for code in ["at", "ch"] {
            let mp = auth_marketplace(code);
            assert_eq!(mp.marketplace_id, "A1PA6795UKMFR9");
            assert_eq!(mp.domain, "amazon.de");
            assert_eq!(mp.region, "de");
        }
    }

    #[test]
    fn auth_marketplace_falls_back_to_us_for_unknown_codes() {
        // `mx` and `in` are real marketplaces but are **not** in the device
        // table, so they fall back to US exactly like the original.
        for code in ["mx", "in", "zz", ""] {
            let mp = auth_marketplace(code);
            assert_eq!(mp.marketplace_id, "ATVPDKIKX0DER", "fallback for {code:?}");
            assert_eq!(mp.domain, "amazon.com", "fallback domain for {code:?}");
            assert_eq!(mp.region, "us", "fallback region for {code:?}");
        }
    }

    #[test]
    fn auth_portal_url_is_parameter_order_exact_for_de() {
        assert_eq!(
            auth_portal_url("de", "device:abc", "CHAL", "de"),
            "https://www.amazon.de/ap/signin?\
openid.ns=http://specs.openid.net/auth/2.0\
&openid.claimed_id=http://specs.openid.net/auth/2.0/identifier_select\
&openid.identity=http://specs.openid.net/auth/2.0/identifier_select\
&openid.mode=checkid_setup\
&openid.oa2.scope=device_auth_access\
&openid.ns.oa2=http://www.amazon.com/ap/ext/oauth/2\
&openid.oa2.response_type=code\
&openid.oa2.code_challenge_method=S256\
&openid.oa2.client_id=device:abc\
&language=de\
&marketPlaceId=A1PA6795UKMFR9\
&openid.return_to=https://www.amazon.de/ap/maplanding\
&openid.pape.max_auth_age=0\
&forceMobileLayout=true\
&openid.assoc_handle=amzn_photos_desktop_de\
&pageId=amzn_photos_desktop_de\
&openid.oa2.code_challenge=CHAL"
        );
    }

    #[test]
    fn auth_portal_url_uses_the_marketplace_domain_and_id() {
        let url = auth_portal_url("uk", "device:xyz", "CH", "en");
        assert!(url.starts_with("https://www.amazon.co.uk/ap/signin?"));
        // The alias `gb` resolves to the same British marketplace.
        assert_eq!(url, auth_portal_url("gb", "device:xyz", "CH", "en"));
        assert!(url.contains("marketPlaceId=A1F83G8C2ARO7P"));
        assert!(url.contains("openid.assoc_handle=amzn_photos_desktop_uk"));
        assert!(url.contains("pageId=amzn_photos_desktop_uk"));
        assert!(url.contains("openid.return_to=https://www.amazon.co.uk/ap/maplanding"));
        // A non-DE marketplace must not leak the German host or handle.
        assert!(!url.contains("amazon.de"));
        assert!(!url.contains("amzn_photos_desktop_de"));
    }

    #[test]
    fn panda_urls_use_the_marketplace_api_host() {
        assert_eq!(panda_base_url("de"), "https://api.amazon.de");
        assert_eq!(panda_base_url("jp"), "https://api.amazon.co.jp");
        // `at`/`ch` follow the German marketplace.
        assert_eq!(panda_base_url("at"), "https://api.amazon.de");
        assert_eq!(panda_register_url("jp"), "https://api.amazon.co.jp/auth/register");
        assert_eq!(panda_register_url("de"), "https://api.amazon.de/auth/register");
        assert_eq!(panda_token_url("de"), "https://api.amazon.de/auth/token");
        assert_eq!(
            panda_deregister_url("de"),
            "https://api.amazon.de/auth/deregister"
        );
    }

    #[test]
    fn panda_auth_cookies_url_encodes_app_metadata_and_sets_the_domain() {
        assert_eq!(
            panda_auth_cookies_url("de", "RT-1", "Amazon Photos", "11.3.0.1d7941b5"),
            "https://api.amazon.de/auth/token?source_token=RT-1\
&source_token_type=refresh_token\
&requested_token_type=auth_cookies\
&domain=.amazon.de\
&app_name=Amazon%20Photos\
&app_version=11.3.0.1d7941b5"
        );
        // The refresh token and the domain stay raw; only the app metadata is
        // percent-encoded.
        let url = panda_auth_cookies_url("uk", "raw+token=", "A B", "1 2");
        assert!(url.contains("source_token=raw+token="));
        assert!(url.contains("domain=.amazon.co.uk"));
        assert!(url.contains("app_name=A%20B"));
        assert!(url.contains("app_version=1%202"));
    }
}
