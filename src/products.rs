//! The Products: Auth's public list of them, cached per Auth host, and the `MVDM_<NAME>_URL` variables that point
//! one Product at another host and win over the list.

use std::collections::BTreeMap;
use std::env;

use serde::Deserialize;

use crate::cache::{self, Cached};
use crate::config;
use crate::failure::{ErrorCode, Failure};
use crate::http;

const LIST_PATH: &str = "/agent-access/apps";
const CACHE_KIND: &str = "products";
const VARIABLE_PREFIX: &str = "MVDM_";
const VARIABLE_SUFFIX: &str = "_URL";
/// `MVDM_AUTH_URL` ends like a Product's variable but names Auth.
const AUTH_NAME: &str = "AUTH";

/// One Product that offers Agents an API.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Product {
    /// The command name, such as `compliance`.
    pub name: String,
    pub display_name: String,
    pub base_url: String,
    /// The REST API resource a token is bound to (RFC 8707).
    pub api_resource: String,
    /// The address of the OpenAPI description.
    pub open_api: String,
    /// The address of the Product's guides; empty when Auth leaves it out, until `parse` fills it in.
    #[serde(default)]
    pub guides: String,
}

impl Product {
    /// A Product named only by a variable: its other addresses are the default paths on `base_url`.
    fn at(name: String, base_url: String) -> Self {
        Product {
            display_name: name.clone(),
            api_resource: format!("{base_url}/api"),
            open_api: format!("{base_url}/openapi/v1.json"),
            guides: default_guides(&base_url),
            name,
            base_url,
        }
    }
}

fn default_guides(base_url: &str) -> String {
    format!("{base_url}/agents")
}

#[derive(Deserialize)]
struct Answer {
    apps: Vec<Product>,
}

/// Auth's list of Products, in Auth's order: the cached list when it is younger than an hour, else a fresh one.
/// When the fetch fails, a stale copy is used; with no copy at all the failure is `discovery`.
pub fn load_list(auth: &str) -> Result<Cached<Vec<Product>>, Failure> {
    Cached::load(cache::file(CACHE_KIND, auth), parse, || fetch(auth))
}

/// GETs the list without a credential, and caches it.
fn fetch(auth: &str) -> Result<Vec<Product>, Failure> {
    let url = format!("{auth}{LIST_PATH}");
    let unreadable = |why: String| {
        Failure::local(
            ErrorCode::Discovery,
            format!("Could not read the Product list at {url}: {why}"),
        )
    };
    let (status, bytes) = match http::read_answer(&url, http::agent().get(&url).call()) {
        Ok(answer) => answer,
        // The transport failure already names the address.
        Err(Failure::Local { message, .. }) => {
            let why = message
                .strip_prefix(&format!("{url}: "))
                .unwrap_or(&message);
            return Err(unreadable(why.to_string()));
        }
        Err(other) => return Err(other),
    };
    if !status.is_success() {
        return Err(unreadable(format!("the server answered {status}.")));
    }
    let products = parse(&bytes).ok_or_else(|| unreadable("it is not a Product list.".into()))?;
    if let Some(path) = cache::file(CACHE_KIND, auth) {
        // Ignored: without a cache the next run fetches again.
        let _ = cache::save(&path, &bytes);
    }
    Ok(products)
}

fn parse(bytes: &[u8]) -> Option<Vec<Product>> {
    let mut products = serde_json::from_slice::<Answer>(bytes).ok()?.apps;
    for product in &mut products {
        // Auth may leave the guides out: they are at the default path on the base address.
        if product.guides.trim().is_empty() {
            product.guides = default_guides(&product.base_url);
        }
    }
    Some(products)
}

/// The Products the environment names: every non-empty `MVDM_<NAME>_URL` except `MVDM_AUTH_URL`, in name order.
/// The name is `<NAME>` lowercased with `_` turned into `-`.
pub fn from_variables() -> Vec<Product> {
    let named: BTreeMap<String, Product> = env::vars_os()
        .filter_map(|(variable, value)| {
            let variable = variable.to_str()?;
            let middle = variable
                .strip_prefix(VARIABLE_PREFIX)?
                .strip_suffix(VARIABLE_SUFFIX)?;
            let base_url = config::trim_address(value.to_str()?);
            if middle.is_empty() || middle == AUTH_NAME || base_url.is_empty() {
                return None;
            }
            let name = middle.to_ascii_lowercase().replace('_', "-");
            Some((name.clone(), Product::at(name, base_url)))
        })
        .collect();
    named.into_values().collect()
}

fn find<'a>(products: &'a [Product], name: &str) -> Option<&'a Product> {
    products.iter().find(|product| product.name == name)
}

/// The Products `mvdm --help` lists: Auth's, in its order, then the ones only a variable names, in name order.
/// When the list cannot be read, the Products a variable names still come, and the failure comes beside them.
pub fn for_help() -> (Vec<Product>, Option<Failure>) {
    let (mut products, failure) = match load_list(&config::auth()) {
        Ok(list) => (list.value, None),
        Err(failure) => (Vec::new(), Some(failure)),
    };
    for product in from_variables() {
        if !products.iter().any(|listed| listed.name == product.name) {
            products.push(product);
        }
    }
    (products, failure)
}

/// The Product a command line names. A variable names it with no look at Auth; otherwise the list does. A name
/// the list lacks forces one fresh fetch of the list before it is reported as unknown (a usage mistake).
pub fn named(name: &str) -> Result<Product, Failure> {
    let auth = config::auth();
    let variables = from_variables();
    if let Some(product) = variables.iter().find(|product| product.name == name) {
        return Ok(with_listed_display_name(
            product.clone(),
            &cached_list(&auth),
        ));
    }
    let mut list = load_list(&auth)?;
    if find(&list.value, name).is_none() {
        list.refetch(|| fetch(&auth));
    }
    match find(&list.value, name) {
        Some(product) => Ok(product.clone()),
        None => {
            let known: Vec<&str> = list
                .value
                .iter()
                .chain(&variables)
                .map(|product| product.name.as_str())
                .collect();
            Err(Failure::Usage(format!(
                "Unknown Product {name}. The Products are: {}. Run `mvdm --help` to list them.",
                known.join(", ")
            )))
        }
    }
}

/// A Product a variable names takes the display name the list gives it. The variable decides the rest.
fn with_listed_display_name(mut product: Product, listed: &[Product]) -> Product {
    if let Some(listed) = find(listed, &product.name) {
        product.display_name = listed.display_name.clone();
    }
    product
}

/// The cached list however old it is, with no fetch.
fn cached_list(auth: &str) -> Vec<Product> {
    cache::file(CACHE_KIND, auth)
        .and_then(|path| cache::read(&path, parse))
        .map(|(products, _)| products)
        .unwrap_or_default()
}

/// The Product `status` and `accounts` ask for the Account: the first named by a variable, in name order, else
/// the first one Auth lists. The list is fetched only without a variable.
pub fn account_product() -> Result<Product, Failure> {
    if let Some(product) = from_variables().into_iter().next() {
        return Ok(product);
    }
    let auth = config::auth();
    first_listed(&auth)
}

/// The API resource a sign-in names: the first listed Product's, never one a variable names.
pub fn sign_in_resource(auth: &str) -> Result<String, Failure> {
    first_listed(auth).map(|product| product.api_resource)
}

fn first_listed(auth: &str) -> Result<Product, Failure> {
    load_list(auth)?.value.into_iter().next().ok_or_else(|| {
        Failure::local(
            ErrorCode::Discovery,
            format!("The Product list at {auth}{LIST_PATH} is empty."),
        )
    })
}
