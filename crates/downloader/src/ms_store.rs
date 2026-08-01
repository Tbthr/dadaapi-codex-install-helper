use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::{SecondsFormat, Utc};
use reqwest::{header::CONTENT_DISPOSITION, redirect::Policy, Client};
use roxmltree::{Document, Node};
use serde::Deserialize;
use thiserror::Error;
use url::Url;
use uuid::Uuid;

use shared_types::CpuArchitecture;

const PRODUCT_ID: &str = "9PLM9XGG6VKS";
const STORE_PRODUCT_URL: &str = "https://storeedgefd.dsx.mp.microsoft.com/v9.0/products";
const WU_ENDPOINT: &str = "https://fe3.delivery.mp.microsoft.com/ClientWebService/client.asmx";
const WU_SECURED_ENDPOINT: &str =
    "https://fe3cr.delivery.mp.microsoft.com/ClientWebService/client.asmx/secured";
const WU_NS: &str = "http://www.microsoft.com/SoftwareDistribution/Server/ClientWebService";
const REMOTE_MANIFEST_URL: &str =
    "https://gitee.com/lyq_power/dadaapi-codex-install-helper/raw/msix-links/msix-links.json";
const GITEE_RAW_CONTENT_HOST: &str = "raw.giteeusercontent.com";
const MICROSOFT_DELIVERY_HOST: &str = "dl.delivery.mp.microsoft.com";
const MAX_REMOTE_MANIFEST_BYTES: usize = 64 * 1024;

const INSTALLED_NON_LEAF_IDS: &str = "1,2,3,11,19,544,549,2359974,2359977,5169044,8788830,23110993,23110994,54341900,54343656,59830006,59830007,59830008,60484010,62450018,62450019,62450020,66027979,66053150,97657898,98822896,98959022,98959023,98959024,98959025,98959026,104433538,104900364,105489019,117765322,129905029,130040031,132387090,132393049,133399034,138537048,140377312,143747671,158941041,158941042,158941043,158941044,159123858,159130928,164836897,164847386,164848327,164852241,164852246,164852252,164852253";

const OTHER_CACHED_IDS: &str = "10,17,2359977,5143990,5169043,5169047,8806526,9125350,9154769,10809856,23110995,23110996,23110999,23111000,23111001,23111002,23111003,23111004,24513870,28880263,30077688,30486944,30526991,30528442,30530496,30530501,30530504,30530962,30535326,30536242,30539913,30545142,30545145,30545488,30546212,30547779,30548797,30548860,30549262,30551160,30551161,30551164,30553016,30553744,30554014,30559008,30559011,30560006,30560011,30561006,30563261,30565215,30578059,30664998,30677904,30681618,30682195,30685055,30702579,30708772,30709591,30711304,30715418,30720106,30720273,30732075,30866952,30866964,30870749,30877852,30878437,30890151,30892149,30990917,31049444,31190936,31196961,31197811,31198836,31202713,31203522,31205442,31205557,31207585,31208440,31208451,31209591,31210536,31211625,31212713,31213588,31218518,31219420,31220279,31220302,31222086,31227080,31229030,31238236,31254198,31258008,36436779,36437850,36464012,41916569,47249982,47283134,58577027,58578040,58578041,58628920,59107045,59125697,59142249,60466586,60478936,66450441,66467021,66479051,75202978,77436021,77449129,85159569,90199702,90212090,96911147,97110308,98528428,98665206,98837995,98842922,98842977,98846632,98866485,98874250,98879075,98904649,98918872,98945691,98959458,98984707,100220125,100238731,100662329,100795834,100862457,103124811,103348671,104369981,104372472,104385324,104465831,104465834,104467697,104473368,104482267,104505005,104523840,104550085,104558084,104659441,104659675,104664678,104668274,104671092,104673242,104674239,104679268,104686047,104698649,104751469,104752478,104755145,104761158,104762266,104786484,104853747,104873258,104983051,105063056,105116588,105178523,105318602,105362613,105364552,105368563,105369591,105370746,105373503,105373615,105376634,105377546,105378752,105379574,105381626,105382587,105425313,105495146,105862607,105939029,105995585,106017178,106129726,106768485,107825194,111906429,115121473,115578654,116630363,117835105,117850671,118638500,118662027,118872681,118873829,118879289,118889092,119501720,119551648,119569538,119640702,119667998,119674103,119697201,119706266,119744627,119773746,120072697,120144309,120214154,120357027,120392612,120399120,120553945,120783545,120797092,120881676,120889689,120999554,121168608,121268830,121341838,121729951,121803677,122165810,125408034,127293130,127566683,127762067,127861893,128571722,128647535,128698922,128701748,128771507,129037212,129079800,129175415,129317272,129319665,129365668,129378095,129424803,129590730,129603714,129625954,129692391,129714980,129721097,129886397,129968371,129972243,130009862,130033651,130040030,130040032,130040033,130091954,130100640,130131267,130131921,130144837,130171030,130172071,130197218,130212435,130291076,130402427,130405166,130676169,130698471,130713390,130785217,131396908,131455115,131682095,131689473,131701956,132142800,132525441,132765492,132801275,133399034,134522926,134524022,134528994,134532942,134536993,134538001,134547533,134549216,134549317,134550159,134550214,134550232,134551154,134551207,134551390,134553171,134553237,134554199,134554227,134555229,134555240,134556118,134557078,134560099,134560287,134562084,134562180,134563287,134565083,134566130,134568111,134624737,134666461,134672998,134684008,134916523,135100527,135219410,135222083,135306997,135463054,135779456,135812968,136097030,136131333,136146907,136157556,136320962,136450641,136466000,136745792,136761546,136840245,138160034,138181244,138210071,138210107,138232200,138237088,138277547,138287133,138306991,138324625,138341916,138372035,138372036,138375118,138378071,138380128,138380194,138534411,138618294,138931764,139536037,139536038,139536039,139536040,140367832,140406050,140421668,140422973,140423713,140436348,140483470,140615715,140802803,140896470,141189437,141192744,141382548,141461680,141624996,141627135,141659139,141872038,141993721,142006413,142045136,142095667,142227273,142250480,142518788,142544931,142546314,142555433,142653044,143191852,143258496,143299722,143331253,143432462,143632431,143695326,144219522,144590916,145410436,146720405,150810438,151258773,151315554,151400090,151429441,151439617,151453617,151466296,151511132,151636561,151823192,151827116,151850642,152016572,153111675,153114652,153123147,153267108,153389799,153395366,153718608,154171028,154315227,154559688,154978771,154979742,154985773,154989370,155044852,155065458,155578573,156403304,159085959,159776047,159816630,160733048,160733049,160733050,160733051,160733056,164824922,164824924,164824926,164824930,164831646,164831647,164831648,164831650,164835050,164835051,164835052,164835056,164835057,164835059,164836898,164836899,164836900,164845333,164845334,164845336,164845337,164845341,164845342,164845345,164845346,164845349,164845350,164845353,164845355,164845358,164845361,164845364,164847387,164847388,164847389,164847390,164848328,164848329,164848330,164849448,164849449,164849451,164849452,164849454,164849455,164849457,164849461,164850219,164850220,164850222,164850223,164850224,164850226,164850227,164850228,164850229,164850231,164850236,164850237,164850240,164850242,164850243,164852242,164852243,164852244,164852247,164852248,164852249,164852250,164852251,164852254,164852256,164852257,164852258,164852259,164852260,164852261,164852262,164853061,164853063,164853071,164853072,164853075,168118980,168118981,168118983,168118984,168180375,168180376,168180378,168180379,168270830,168270831,168270833,168270834,168270835";

#[derive(Debug, Error)]
pub enum MsStoreError {
    #[error("Microsoft Store request failed")]
    Request,
    #[error("Microsoft Store returned invalid metadata")]
    InvalidMetadata,
    #[error("Microsoft Store has no package for this architecture")]
    ArchitectureNotFound,
}

#[derive(Debug, Deserialize)]
struct StoreProduct {
    #[serde(rename = "Payload")]
    payload: StorePayload,
}

#[derive(Debug, Deserialize)]
struct StorePayload {
    #[serde(rename = "Skus", default)]
    skus: Vec<StoreSku>,
}

#[derive(Debug, Deserialize)]
struct StoreSku {
    #[serde(rename = "FulfillmentData")]
    fulfillment_data: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct FulfillmentData {
    #[serde(rename = "WuBundleId")]
    bundle_id: String,
    #[serde(rename = "WuCategoryId")]
    category_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct UpdateIdentity {
    id: String,
    revision: String,
}

#[derive(Debug, Clone)]
struct UpdateRecord {
    identity: UpdateIdentity,
    bundled: Vec<UpdateIdentity>,
}

#[derive(Debug)]
struct Candidate {
    url: Url,
    architecture: Option<CpuArchitecture>,
}

pub async fn resolve_chatgpt_msix_url(architecture: CpuArchitecture) -> Result<Url, MsStoreError> {
    match resolve_chatgpt_msix_url_direct(architecture).await {
        Ok(url) => Ok(url),
        Err(error) => {
            tracing::warn!(error = %error, "local Microsoft Store resolution failed; using refreshed Gitee metadata");
            resolve_remote_msix_url(architecture).await
        }
    }
}

pub async fn resolve_chatgpt_msix_url_direct(
    architecture: CpuArchitecture,
) -> Result<Url, MsStoreError> {
    let client = metadata_client()?;
    let fulfillment = fetch_fulfillment(&client).await?;
    let cookie = get_cookie(&client).await?;
    let sync = post_soap(
        &client,
        WU_ENDPOINT,
        &format!("{WU_NS}/SyncUpdates"),
        &build_sync_updates_xml(&cookie, &fulfillment.category_id),
    )
    .await?;
    let records = collect_update_records(&sync)?;
    let bundle = records
        .values()
        .find(|record| {
            record
                .identity
                .id
                .eq_ignore_ascii_case(&fulfillment.bundle_id)
        })
        .ok_or(MsStoreError::InvalidMetadata)?;
    let leaves = find_leaves(bundle, &records);
    if leaves.is_empty() {
        return Err(MsStoreError::InvalidMetadata);
    }

    let mut candidates = Vec::new();
    for leaf in leaves {
        let response = post_soap(
            &client,
            WU_SECURED_ENDPOINT,
            &format!("{WU_NS}/GetExtendedUpdateInfo2"),
            &build_extended_info_xml(&leaf),
        )
        .await?;
        let Some(raw_url) = extract_file_url(&response)? else {
            continue;
        };
        let url = Url::parse(&raw_url).map_err(|_| MsStoreError::InvalidMetadata)?;
        let file_name = head_file_name(&client, url.clone()).await;
        candidates.push(Candidate {
            architecture: file_name.as_deref().and_then(detect_architecture),
            url,
        });
    }

    candidates
        .iter()
        .find(|candidate| candidate.architecture == Some(architecture))
        .or_else(|| {
            candidates
                .iter()
                .find(|candidate| candidate.architecture.is_none())
        })
        .map(|candidate| candidate.url.clone())
        .ok_or(MsStoreError::ArchitectureNotFound)
}

#[derive(Debug, Deserialize)]
struct RemoteManifest {
    packages: RemotePackages,
}

#[derive(Debug, Deserialize)]
struct RemotePackages {
    arm64: RemotePackage,
    x64: RemotePackage,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemotePackage {
    url: String,
    expires_at: chrono::DateTime<Utc>,
}

async fn resolve_remote_msix_url(architecture: CpuArchitecture) -> Result<Url, MsStoreError> {
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(15))
        .redirect(Policy::custom(|attempt| {
            if attempt.previous().len() > 1 {
                return attempt.error("too many MSIX metadata redirects");
            }
            if trusted_remote_manifest_redirect(attempt.url()) {
                attempt.follow()
            } else {
                attempt.error("untrusted MSIX metadata redirect")
            }
        }))
        .build()
        .map_err(|_| MsStoreError::Request)?;
    let response = client
        .get(REMOTE_MANIFEST_URL)
        .header("Accept", "application/json")
        .header("User-Agent", "dada-assistant/1.0")
        .send()
        .await
        .map_err(|_| MsStoreError::Request)?
        .error_for_status()
        .map_err(|_| MsStoreError::Request)?;
    if response
        .content_length()
        .is_some_and(|size| size > MAX_REMOTE_MANIFEST_BYTES as u64)
    {
        return Err(MsStoreError::InvalidMetadata);
    }
    let body = response.bytes().await.map_err(|_| MsStoreError::Request)?;
    if body.len() > MAX_REMOTE_MANIFEST_BYTES {
        return Err(MsStoreError::InvalidMetadata);
    }
    let manifest = serde_json::from_slice::<RemoteManifest>(&body)
        .map_err(|_| MsStoreError::InvalidMetadata)?;
    let package = match architecture {
        CpuArchitecture::Arm64 => manifest.packages.arm64,
        CpuArchitecture::X64 => manifest.packages.x64,
    };
    if package.expires_at <= Utc::now() + chrono::Duration::minutes(5) {
        return Err(MsStoreError::InvalidMetadata);
    }
    let url = Url::parse(&package.url).map_err(|_| MsStoreError::InvalidMetadata)?;
    if !trusted_microsoft_delivery_url(&url) {
        return Err(MsStoreError::InvalidMetadata);
    }
    Ok(url)
}

fn trusted_remote_manifest_redirect(url: &Url) -> bool {
    url.scheme() == "https"
        && url.host_str() == Some(GITEE_RAW_CONTENT_HOST)
        && url.username().is_empty()
        && url.password().is_none()
        && url.fragment().is_none()
        && url.path() == "/lyq_power/dadaapi-codex-install-helper/raw/msix-links/msix-links.json"
}

fn trusted_microsoft_delivery_url(url: &Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    let host_allowed = host == MICROSOFT_DELIVERY_HOST
        || host
            .strip_suffix(MICROSOFT_DELIVERY_HOST)
            .is_some_and(|prefix| prefix.ends_with('.') && !prefix[..prefix.len() - 1].is_empty());
    (url.scheme() == "https" || url.scheme() == "http")
        && url.username().is_empty()
        && url.password().is_none()
        && url.fragment().is_none()
        && host_allowed
}

fn metadata_client() -> Result<Client, MsStoreError> {
    Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .redirect(Policy::none())
        .build()
        .map_err(|_| MsStoreError::Request)
}

async fn fetch_fulfillment(client: &Client) -> Result<FulfillmentData, MsStoreError> {
    let url = format!(
        "{STORE_PRODUCT_URL}/{PRODUCT_ID}?market=US&locale=en-us&deviceFamily=Windows.Desktop"
    );
    let response = send_store_product_request(client, &url)
        .await
        .map_err(|_| MsStoreError::Request)?;
    let product = response
        .error_for_status()
        .map_err(|_| MsStoreError::Request)?
        .json::<StoreProduct>()
        .await
        .map_err(|_| MsStoreError::InvalidMetadata)?;

    for sku in product.payload.skus {
        let parsed = match sku.fulfillment_data {
            serde_json::Value::String(value) => serde_json::from_str(&value),
            value => serde_json::from_value(value),
        };
        if let Ok(data) = parsed {
            return Ok(data);
        }
    }
    Err(MsStoreError::InvalidMetadata)
}

async fn send_store_product_request(
    client: &Client,
    url: &str,
) -> Result<reqwest::Response, reqwest::Error> {
    client
        .get(url)
        .header("Accept", "application/json")
        .header("User-Agent", "dada-assistant/1.0")
        .send()
        .await
}

async fn get_cookie(client: &Client) -> Result<String, MsStoreError> {
    let response = post_soap(
        client,
        WU_ENDPOINT,
        &format!("{WU_NS}/GetCookie"),
        &build_get_cookie_xml(),
    )
    .await?;
    let document = Document::parse(&response).map_err(|_| MsStoreError::InvalidMetadata)?;
    document
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "EncryptedData")
        .and_then(|node| node.text())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or(MsStoreError::InvalidMetadata)
}

async fn post_soap(
    client: &Client,
    endpoint: &str,
    action: &str,
    body: &str,
) -> Result<String, MsStoreError> {
    let response = send_soap_request(client, endpoint, action, body)
        .await
        .map_err(|_| MsStoreError::Request)?;
    response
        .error_for_status()
        .map_err(|_| MsStoreError::Request)?
        .text()
        .await
        .map_err(|_| MsStoreError::Request)
}

async fn send_soap_request(
    client: &Client,
    endpoint: &str,
    action: &str,
    body: &str,
) -> Result<reqwest::Response, reqwest::Error> {
    client
        .post(endpoint)
        .header(
            "Content-Type",
            format!("application/soap+xml; charset=utf-8; action=\"{action}\""),
        )
        .header(
            "User-Agent",
            "Windows-Update-Agent/10.0.10011.16384 Client-Protocol/2.50",
        )
        .body(body.to_owned())
        .send()
        .await
}

async fn head_file_name(client: &Client, url: Url) -> Option<String> {
    let response = client
        .head(url)
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?;
    let disposition = response.headers().get(CONTENT_DISPOSITION)?.to_str().ok()?;
    file_name_from_disposition(disposition)
}

fn file_name_from_disposition(value: &str) -> Option<String> {
    let encoded = value
        .split(';')
        .map(str::trim)
        .find_map(|part| part.strip_prefix("filename*=UTF-8''"));
    if let Some(encoded) = encoded {
        return percent_encoding::percent_decode_str(encoded)
            .decode_utf8()
            .ok()
            .map(|value| value.into_owned());
    }
    value.split(';').map(str::trim).find_map(|part| {
        part.strip_prefix("filename=")
            .map(|name| name.trim_matches('"').to_owned())
    })
}

fn detect_architecture(file_name: &str) -> Option<CpuArchitecture> {
    let value = file_name.to_ascii_lowercase();
    if value.contains("_arm64__") {
        Some(CpuArchitecture::Arm64)
    } else if value.contains("_x64__") {
        Some(CpuArchitecture::X64)
    } else {
        None
    }
}

#[cfg(test)]
mod remote_manifest_tests {
    use super::*;

    #[test]
    fn remote_manifest_uses_the_public_gitee_repository() {
        let url = Url::parse(REMOTE_MANIFEST_URL).expect("remote manifest URL");
        assert_eq!(url.scheme(), "https");
        assert_eq!(url.host_str(), Some("gitee.com"));
        assert_eq!(
            url.path(),
            "/lyq_power/dadaapi-codex-install-helper/raw/msix-links/msix-links.json"
        );
    }

    #[test]
    fn only_accepts_the_expected_gitee_content_redirect() {
        assert!(trusted_remote_manifest_redirect(
            &Url::parse(
                "https://raw.giteeusercontent.com/lyq_power/dadaapi-codex-install-helper/raw/msix-links/msix-links.json?signature=test"
            )
            .expect("URL")
        ));
        assert!(!trusted_remote_manifest_redirect(
            &Url::parse(
                "https://raw.giteeusercontent.com/other/repository/raw/msix-links/msix-links.json"
            )
            .expect("URL")
        ));
        assert!(!trusted_remote_manifest_redirect(
            &Url::parse(
                "http://raw.giteeusercontent.com/lyq_power/dadaapi-codex-install-helper/raw/msix-links/msix-links.json"
            )
            .expect("URL")
        ));
    }

    #[test]
    fn only_accepts_safe_microsoft_delivery_urls() {
        assert!(trusted_microsoft_delivery_url(
            &Url::parse("http://tlu.dl.delivery.mp.microsoft.com/file?P1=1").expect("URL")
        ));
        assert!(trusted_microsoft_delivery_url(
            &Url::parse("https://dl.delivery.mp.microsoft.com/file?P1=1").expect("URL")
        ));
        assert!(!trusted_microsoft_delivery_url(
            &Url::parse("https://example.com/file?P1=1").expect("URL")
        ));
        assert!(!trusted_microsoft_delivery_url(
            &Url::parse("https://user@dl.delivery.mp.microsoft.com/file?P1=1").expect("URL")
        ));
    }
}

fn collect_update_records(xml: &str) -> Result<HashMap<String, UpdateRecord>, MsStoreError> {
    let document = Document::parse(xml).map_err(|_| MsStoreError::InvalidMetadata)?;
    let mut records = HashMap::new();
    collect_records_from_root(document.root_element(), &mut records);

    for xml_node in document
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "Xml")
    {
        let Some(fragment) = xml_node
            .text()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let fragment = strip_xml_declaration(fragment);
        let wrapped = format!("<Root>{fragment}</Root>");
        if let Ok(fragment_document) = Document::parse(&wrapped) {
            collect_records_from_root(fragment_document.root_element(), &mut records);
        }
    }
    Ok(records)
}

fn collect_records_from_root(root: Node<'_, '_>, records: &mut HashMap<String, UpdateRecord>) {
    for node in root.descendants().filter(Node::is_element) {
        let Some(identity) = normalize_identity(node) else {
            continue;
        };
        let bundled = collect_bundled_identities(node);
        let key = identity_key(&identity);
        let replace = records
            .get(&key)
            .is_none_or(|previous| bundled.len() > previous.bundled.len());
        if replace {
            records.insert(key, UpdateRecord { identity, bundled });
        }
    }
}

fn normalize_identity(node: Node<'_, '_>) -> Option<UpdateIdentity> {
    let target = if node.tag_name().name() == "UpdateIdentity" {
        node
    } else {
        direct_child(node, "UpdateIdentity").unwrap_or(node)
    };
    let id = direct_child_text(target, "UpdateID")
        .or_else(|| direct_child_text(target, "ID"))
        .or_else(|| target.attribute("UpdateID").map(str::to_owned))
        .or_else(|| target.attribute("ID").map(str::to_owned))?;
    let revision = direct_child_text(target, "RevisionNumber")
        .or_else(|| direct_child_text(target, "Revision"))
        .or_else(|| target.attribute("RevisionNumber").map(str::to_owned))
        .or_else(|| target.attribute("Revision").map(str::to_owned))?;
    Some(UpdateIdentity { id, revision })
}

fn collect_bundled_identities(node: Node<'_, '_>) -> Vec<UpdateIdentity> {
    let container = direct_child(node, "Relationships")
        .and_then(|relationships| direct_child(relationships, "BundledUpdates"))
        .or_else(|| direct_child(node, "BundledUpdates"))
        .or_else(|| direct_child(node, "PayloadFiles"));
    let Some(container) = container else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    container
        .descendants()
        .filter_map(normalize_identity)
        .filter(|identity| seen.insert(identity_key(identity)))
        .collect()
}

fn find_leaves(
    bundle: &UpdateRecord,
    records: &HashMap<String, UpdateRecord>,
) -> Vec<UpdateIdentity> {
    fn visit(
        record: &UpdateRecord,
        records: &HashMap<String, UpdateRecord>,
        seen: &mut HashSet<String>,
        leaves: &mut Vec<UpdateIdentity>,
    ) {
        if !seen.insert(identity_key(&record.identity)) {
            return;
        }
        if record.bundled.is_empty() {
            leaves.push(record.identity.clone());
            return;
        }
        for identity in &record.bundled {
            if let Some(child) = records.get(&identity_key(identity)) {
                visit(child, records, seen, leaves);
            } else {
                leaves.push(identity.clone());
            }
        }
    }

    let mut leaves = Vec::new();
    visit(bundle, records, &mut HashSet::new(), &mut leaves);
    leaves
}

fn extract_file_url(xml: &str) -> Result<Option<String>, MsStoreError> {
    let document = Document::parse(xml).map_err(|_| MsStoreError::InvalidMetadata)?;
    let mut urls = Vec::new();
    for node in document.descendants().filter(Node::is_element) {
        for key in ["Url", "URL", "url"] {
            if let Some(value) = node
                .attribute(key)
                .filter(|value| value.starts_with("http"))
            {
                urls.push(value.to_owned());
            }
        }
        if matches!(node.tag_name().name(), "FileLocation" | "Url" | "URL") {
            if let Some(value) = node
                .text()
                .map(str::trim)
                .filter(|value| value.starts_with("http"))
            {
                urls.push(value.to_owned());
            }
        }
    }
    Ok(urls
        .iter()
        .find(|url| url.contains("?P1="))
        .or_else(|| {
            urls.iter()
                .find(|url| url.contains("dl.delivery.mp.microsoft.com"))
        })
        .cloned())
}

fn direct_child<'a, 'input>(node: Node<'a, 'input>, name: &str) -> Option<Node<'a, 'input>> {
    node.children()
        .find(|child| child.is_element() && child.tag_name().name() == name)
}

fn direct_child_text(node: Node<'_, '_>, name: &str) -> Option<String> {
    direct_child(node, name)
        .and_then(|child| child.text())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn identity_key(identity: &UpdateIdentity) -> String {
    format!("{}:{}", identity.id, identity.revision).to_ascii_lowercase()
}

fn strip_xml_declaration(value: &str) -> &str {
    if value.starts_with("<?xml") {
        value.find("?>").map_or(value, |index| &value[index + 2..])
    } else {
        value
    }
}

fn build_get_cookie_xml() -> String {
    let now = iso_now();
    soap_envelope(
        &format!("{WU_NS}/GetCookie"),
        WU_ENDPOINT,
        &format!(
            r#"<GetCookie xmlns="{WU_NS}">
  <oldCookie><Expiration>{now}</Expiration></oldCookie>
  <lastChange>{now}</lastChange>
  <currentTime>{now}</currentTime>
  <protocolVersion>2.0</protocolVersion>
</GetCookie>"#
        ),
    )
}

fn build_sync_updates_xml(cookie: &str, category_id: &str) -> String {
    let expiration =
        (Utc::now() + chrono::Duration::hours(1)).to_rfc3339_opts(SecondsFormat::Secs, true);
    soap_envelope(
        &format!("{WU_NS}/SyncUpdates"),
        WU_ENDPOINT,
        &format!(
            r#"<SyncUpdates xmlns="{WU_NS}">
  <cookie><Expiration>{expiration}</Expiration><EncryptedData>{}</EncryptedData></cookie>
  <parameters>
    <ExpressQuery>false</ExpressQuery>
    <InstalledNonLeafUpdateIDs>{}</InstalledNonLeafUpdateIDs>
    <OtherCachedUpdateIDs>{}</OtherCachedUpdateIDs>
    <SkipSoftwareSync>false</SkipSoftwareSync>
    <NeedTwoGroupOutOfScopeUpdates>true</NeedTwoGroupOutOfScopeUpdates>
    <FilterAppCategoryIds><CategoryIdentifier><Id>{}</Id></CategoryIdentifier></FilterAppCategoryIds>
    <TreatAppCategoryIdsAsInstalled>true</TreatAppCategoryIdsAsInstalled>
    <AlsoPerformRegularSync>false</AlsoPerformRegularSync>
    <ComputerSpec></ComputerSpec>
    <ExtendedUpdateInfoParameters>
      <XmlUpdateFragmentTypes><XmlUpdateFragmentType>Extended</XmlUpdateFragmentType></XmlUpdateFragmentTypes>
      <Locales><string>en-US</string><string>en</string></Locales>
    </ExtendedUpdateInfoParameters>
    <ClientPreferredLanguages><string>en-US</string></ClientPreferredLanguages>
    <ProductsParameters>
      <SyncCurrentVersionOnly>false</SyncCurrentVersionOnly>
      <DeviceAttributes>{}</DeviceAttributes>
      <CallerAttributes>Interactive=1;IsSeeker=0;</CallerAttributes>
      <Products></Products>
    </ProductsParameters>
  </parameters>
</SyncUpdates>"#,
            escape_xml(cookie),
            render_int_list(INSTALLED_NON_LEAF_IDS),
            render_int_list(OTHER_CACHED_IDS),
            escape_xml(category_id),
            device_attributes(),
        ),
    )
}

fn build_extended_info_xml(identity: &UpdateIdentity) -> String {
    soap_envelope(
        &format!("{WU_NS}/GetExtendedUpdateInfo2"),
        WU_SECURED_ENDPOINT,
        &format!(
            r#"<GetExtendedUpdateInfo2 xmlns="{WU_NS}">
  <updateIDs><UpdateIdentity><UpdateID>{}</UpdateID><RevisionNumber>{}</RevisionNumber></UpdateIdentity></updateIDs>
  <infoTypes><XmlUpdateFragmentType>FileUrl</XmlUpdateFragmentType><XmlUpdateFragmentType>FileDecryption</XmlUpdateFragmentType></infoTypes>
  <deviceAttributes>{}</deviceAttributes>
  <locales><string>en-US</string></locales>
</GetExtendedUpdateInfo2>"#,
            escape_xml(&identity.id),
            escape_xml(&identity.revision),
            device_attributes(),
        ),
    )
}

fn soap_envelope(action: &str, endpoint: &str, body: &str) -> String {
    let created = Utc::now();
    let expires = created + chrono::Duration::minutes(2);
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:a="http://www.w3.org/2005/08/addressing" xmlns:s="http://www.w3.org/2003/05/soap-envelope">
  <s:Header>
    <a:Action s:mustUnderstand="1">{}</a:Action>
    <a:MessageID>urn:uuid:{}</a:MessageID>
    <a:To s:mustUnderstand="1">{}</a:To>
    <o:Security s:mustUnderstand="1" xmlns:o="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-secext-1.0.xsd">
      <Timestamp xmlns="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-utility-1.0.xsd">
        <Created>{}</Created><Expires>{}</Expires>
      </Timestamp>
      <wuws:WindowsUpdateTicketsToken wsu:id="ClientMSA" xmlns:wsu="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-utility-1.0.xsd" xmlns:wuws="http://schemas.microsoft.com/msus/2014/10/WindowsUpdateAuthorization">
        <TicketType Name="MSA" Version="1.0" Policy="MBI_SSL"><Device>{}</Device></TicketType>
      </wuws:WindowsUpdateTicketsToken>
    </o:Security>
  </s:Header>
  <s:Body>{body}</s:Body>
</s:Envelope>"#,
        escape_xml(action),
        Uuid::new_v4(),
        escape_xml(endpoint),
        created.to_rfc3339_opts(SecondsFormat::Secs, true),
        expires.to_rfc3339_opts(SecondsFormat::Secs, true),
        windows_update_device_token(),
    )
}

fn windows_update_device_token() -> String {
    const HEADER: &[u8] = &[
        0x13, 0x00, 0x30, 0x02, 0xc3, 0x77, 0x04, 0x00, 0x14, 0xd5, 0xbc, 0xac, 0x7a, 0x66, 0xde,
        0x0d, 0x50, 0xbe, 0xdd, 0xf9, 0xbb, 0xa1, 0x6c, 0x87, 0xed, 0xb9, 0xe0, 0x19, 0x89, 0x80,
        0x00,
    ];
    let mut random = [0_u8; 527];
    if getrandom::fill(&mut random).is_err() {
        return String::new();
    }
    let mut ticket = Vec::with_capacity(HEADER.len() + random.len() + 2);
    ticket.extend_from_slice(HEADER);
    ticket.extend_from_slice(&random);
    ticket.extend_from_slice(&[0xb4, 0x01]);
    let raw = format!("t={}&p=", BASE64.encode(ticket));
    let mut nul_separated = Vec::with_capacity(raw.len() * 2);
    for byte in raw.bytes() {
        nul_separated.push(byte);
        nul_separated.push(0);
    }
    BASE64.encode(nul_separated)
}

fn render_int_list(values: &str) -> String {
    values
        .split(',')
        .map(|value| format!("<int>{}</int>", value.trim()))
        .collect()
}

fn device_attributes() -> &'static str {
    "BranchReadinessLevel=CB;CurrentBranch=rs_prerelease;OEMModel=Virtual Machine;FlightRing=WIS;AttrDataVer=21;SystemManufacturer=Microsoft Corporation;InstallLanguage=en-US;OSUILocale=en-US;InstallationType=Client;FlightingBranchName=external;FirmwareVersion=Hyper-V UEFI Release v2.5;SystemProductName=Virtual Machine;OSSkuId=48;FlightContent=Branch;App=WU;OEMName_Uncleaned=Microsoft Corporation;AppVer=10.0.16184.1001;OSArchitecture=AMD64;SystemSKU=None;UpdateManagementGroup=2;IsFlightingEnabled=1;IsDeviceRetailDemo=0;TelemetryLevel=3;OSVersion=10.0.16184.1001;DeviceFamily=Windows.Desktop"
}

fn iso_now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_store_package_architecture() {
        assert_eq!(
            detect_architecture("OpenAI.Codex_26.707.3748.0_arm64__2p2nqsd0c76g0.Msix"),
            Some(CpuArchitecture::Arm64)
        );
        assert_eq!(
            detect_architecture("OpenAI.Codex_26.707.3748.0_x64__2p2nqsd0c76g0.Msix"),
            Some(CpuArchitecture::X64)
        );
    }

    #[tokio::test]
    #[ignore = "live Microsoft Store integration"]
    async fn resolves_live_arm64_msix() {
        let url = resolve_chatgpt_msix_url_direct(CpuArchitecture::Arm64)
            .await
            .expect("arm64 msix");
        assert!(url
            .host_str()
            .is_some_and(|host| host.ends_with("dl.delivery.mp.microsoft.com")));
    }

    #[tokio::test]
    #[ignore = "live refreshed Gitee metadata integration"]
    async fn resolves_remote_arm64_msix() {
        let url = resolve_remote_msix_url(CpuArchitecture::Arm64)
            .await
            .expect("remote arm64 msix");
        assert!(url
            .host_str()
            .is_some_and(|host| host.ends_with("dl.delivery.mp.microsoft.com")));
    }
}
