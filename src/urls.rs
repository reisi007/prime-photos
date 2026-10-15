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
}
