use serde::Deserialize;
use tauri::Manager;

const API_BASE: &str = "https://www.steamgriddb.com/api/v2";

#[derive(Deserialize)]
struct SearchResp {
    data: Vec<GameHit>,
}

#[derive(Deserialize)]
struct GameHit {
    id: u32,
    name: String,
    #[serde(default)]
    types: Vec<String>,
}

#[derive(Deserialize)]
struct HeroResp {
    data: Vec<HeroAsset>,
}

#[derive(Deserialize)]
struct HeroAsset {
    url: String,
}

#[derive(Deserialize)]
struct MsProduct {
    #[serde(rename = "Product")]
    product: MsInner,
}

#[derive(Deserialize)]
struct MsInner {
    #[serde(rename = "LocalizedProperties")]
    localized: Vec<MsLocalized>,
}

#[derive(Deserialize)]
struct MsLocalized {
    #[serde(rename = "Images")]
    images: Vec<MsImage>,
}

#[derive(Deserialize)]
struct MsImage {
    #[serde(rename = "ImagePurpose")]
    purpose: String,
    #[serde(rename = "Uri")]
    uri: String,
}

#[derive(Deserialize)]
struct SteamGameResp {
    data: SteamGameData,
}

#[derive(Deserialize)]
struct SteamGameData {
    id: u32,
}

fn api_key(app: &tauri::AppHandle) -> Option<String> {
    if let Ok(k) = std::env::var("STEAMGRIDDB_API_KEY") {
        let k = k.trim().to_string();
        if !k.is_empty() {
            return Some(k);
        }
    }
    let path = app.path().app_config_dir().ok()?.join("steamgriddb.key");
    let k = std::fs::read_to_string(path).ok()?.trim().to_string();
    (!k.is_empty()).then_some(k)
}

fn slug(name: &str) -> String {
    let s: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    s.trim_matches('-').to_string()
}

fn mime_of(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "image/jpeg"
    } else if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        "image/png"
    } else if bytes.len() > 11 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else {
        "image/jpeg"
    }
}

fn to_data_url(bytes: &[u8]) -> String {
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    format!("data:{};base64,{}", mime_of(bytes), b64)
}

pub async fn game_hero(
    app: &tauri::AppHandle,
    name: &str,
    steam_appid: Option<u32>,
) -> Option<String> {
    let dir = app.path().app_cache_dir().ok()?.join("artwork");
    let _ = std::fs::create_dir_all(&dir);
    let appid = steam_appid;
    // La clave de caché distingue por AppID cuando lo hay (remasters/secuelas con
    // el mismo nombre tienen AppID distinto), o por nombre cuando no es de Steam.
    let cache_key = match appid {
        Some(id) => format!("hero-steam-{id}"),
        None => format!("hero-{}", slug(name)),
    };
    let path = dir.join(&cache_key);

    // Antes de mirar la lista detectable: en acierto de caché no hace falta saber de qué tienda
    // venía el arte, y consultarla costaba una búsqueda que el fondo ya cacheado no necesita.
    if let Ok(bytes) = std::fs::read(&path) {
        if !bytes.is_empty() {
            return Some(to_data_url(&bytes));
        }
    }

    // Orden: arte oficial primero (Steam y Microsoft publican una sola imagen por juego, limpia
    // y actualizada), y SteamGridDB después, que es comunidad sin señal de calidad: decenas de
    // candidatos con 0 votos entre los que no podemos elegir mejor que al azar.
    // La lista detectable se consulta aquí y no antes para no pagar su parseo en acierto de caché.
    let list_art = crate::detect::art_for(app, name).await;
    let client = reqwest::Client::new();
    let store_appid = appid.or_else(|| list_art.as_ref().and_then(|a| a.steam_appid));
    let official = match store_appid {
        Some(id) => steam_hero(&client, app, id).await,
        None => None,
    };
    let official = match official {
        Some(bytes) => Some(bytes),
        None => match list_art.as_ref().and_then(|a| a.xbox_sku.as_deref()) {
            Some(sku) => ms_hero(&client, sku).await,
            None => None,
        },
    };
    let bytes = match official {
        Some(bytes) => bytes,
        None => sgdb_hero(&client, app, name, store_appid).await?,
    };
    if bytes.is_empty() {
        return None;
    }
    let _ = std::fs::write(&path, &bytes);
    Some(to_data_url(&bytes))
}

// URL PÚBLICA del icono del juego para el Rich Presence de Discord. Mismo orden de fuentes que
// game_icon, para que el RPC enseñe exactamente el mismo arte que la app: primero el icono del
// CDN de Discord y, si el juego no está en su lista, SteamGridDB pidiendo SOLO formatos web
// (png/webp, porque Discord no renderiza los .ico) y por último la portada vertical de Steam.
pub async fn game_art_url(
    app: &tauri::AppHandle,
    name: &str,
    steam_appid: Option<u32>,
) -> Option<String> {
    // Sin descargar nada: el RPC solo necesita una URL pública, y esta ya lo es.
    let list_art = crate::detect::art_for(app, name).await;
    if let Some(url) = list_art.as_ref().and_then(|a| a.icon_url.clone()) {
        return Some(url);
    }
    let steam_appid = steam_appid.or_else(|| list_art.and_then(|a| a.steam_appid));
    let client = reqwest::Client::new();
    if let Some(key) = api_key(app) {
        let game_id = match steam_appid {
            Some(id) => sgdb_by_steam(&client, &key, id).await,
            None => search_game(&client, &key, name).await,
        };
        if let Some(gid) = game_id {
            let url = format!(
                "{API_BASE}/icons/game/{gid}?mimes=image/png,image/webp&nsfw=false&humor=false"
            );
            if let Ok(resp) = client.get(url).bearer_auth(&key).send().await {
                if resp.status().is_success() {
                    if let Ok(parsed) = resp.json::<HeroResp>().await {
                        if let Some(u) = parsed.data.into_iter().next().map(|a| a.url) {
                            return Some(u);
                        }
                    }
                }
            }
        }
    }
    steam_appid.map(|id| {
        format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{id}/library_600x900.jpg")
    })
}

async fn steam_hero(client: &reqwest::Client, app: &tauri::AppHandle, appid: u32) -> Option<Vec<u8>> {
    // library_hero primero: es el fondo oficial de la biblioteca de Steam (1920x620, sin logo y
    // actualizado por la editora en cada update grande), o sea exactamente el formato que
    // pintamos. header.jpg queda de último recurso: es el capsule de tienda de 460x215 con el
    // logo incrustado y, estirado a lo ancho, se ve fatal.
    let candidates = [
        format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{appid}/library_hero.jpg"),
        format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{appid}/header.jpg"),
    ];
    for url in &candidates {
        if let Ok(resp) = client.get(url).send().await {
            if resp.status().is_success() {
                if let Ok(bytes) = resp.bytes().await {
                    if !bytes.is_empty() {
                        return Some(bytes.to_vec());
                    }
                }
            }
        }
    }
    // Fallback: SteamGridDB por AppID (juego exacto, sin ambigüedad de nombre).
    let key = api_key(app)?;
    let game_id = sgdb_by_steam(client, &key, appid).await?;
    let hero_url = first_hero(client, &key, game_id).await?;
    download(client, &hero_url).await
}

// Arte oficial ancho del catálogo de la Microsoft Store. No necesita API key, es una imagen
// canónica por juego (frente a las decenas sin votar de SteamGridDB) y el CDN admite el recorte
// en la propia URL, así que se descarga ya al tamaño que se pinta en vez de un 4K entero.
// Se pide SuperHeroArt y no TitledHeroArt a propósito: el segundo trae el logo incrustado.
const MS_CATALOG: &str = "https://displaycatalog.mp.microsoft.com/v7.0/products";

async fn ms_hero(client: &reqwest::Client, sku: &str) -> Option<Vec<u8>> {
    // market/languages fijos: el catálogo localiza el arte por región y no queremos que el
    // fondo cambie según dónde esté el usuario.
    let url = format!("{MS_CATALOG}/{sku}?market=US&languages=en-us&MS-CV=x");
    let resp = client.get(url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let parsed: MsProduct = resp.json().await.ok()?;
    let uri = parsed
        .product
        .localized
        .into_iter()
        .flat_map(|l| l.images)
        .find(|i| i.purpose == "SuperHeroArt")
        .map(|i| i.uri)?;
    let uri = uri.strip_prefix("//").map(|u| format!("https://{u}")).unwrap_or(uri);
    download(client, &format!("{uri}?w=1920&h=620&format=jpg&q=85&mode=crop")).await
}

async fn sgdb_hero(
    client: &reqwest::Client,
    app: &tauri::AppHandle,
    name: &str,
    appid: Option<u32>,
) -> Option<Vec<u8>> {
    let key = api_key(app)?;
    // Con AppID la búsqueda es exacta; sin él hay que ir por nombre.
    let game_id = match appid {
        Some(id) => sgdb_by_steam(client, &key, id).await?,
        None => search_game(client, &key, name).await?,
    };
    let hero_url = first_hero(client, &key, game_id).await?;
    download(client, &hero_url).await
}

async fn download(client: &reqwest::Client, url: &str) -> Option<Vec<u8>> {
    let bytes = client.get(url).send().await.ok()?.bytes().await.ok()?.to_vec();
    (!bytes.is_empty()).then_some(bytes)
}

async fn sgdb_by_steam(client: &reqwest::Client, key: &str, appid: u32) -> Option<u32> {
    let url = format!("{API_BASE}/games/steam/{appid}");
    let resp = client.get(url).bearer_auth(key).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let parsed: SteamGameResp = resp.json().await.ok()?;
    Some(parsed.data.id)
}

async fn search_game(client: &reqwest::Client, key: &str, name: &str) -> Option<u32> {
    let url = format!("{API_BASE}/search/autocomplete/{}", urlencoding::encode(name));
    let resp = client.get(url).bearer_auth(key).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let parsed: SearchResp = resp.json().await.ok()?;
    pick_game(&parsed.data, name)
}

fn pick_game(data: &[GameHit], name: &str) -> Option<u32> {
    let lower = name.to_lowercase();
    // Entre entradas con el mismo nombre (p. ej. God of War 2005 vs 2018) preferir la
    // versión de PC: la que tiene plataforma (Steam u otra), no la de consola.
    let best_exact = data
        .iter()
        .filter(|g| g.name.to_lowercase() == lower)
        .max_by_key(|g| {
            let steam = g.types.iter().any(|t| t == "steam");
            (steam, !g.types.is_empty())
        });
    best_exact.or_else(|| data.first()).map(|g| g.id)
}

// Sin filtrar por dimensiones: se probó preferir 3840x1240 y salió mal. La resolución no mide
// lo bien que un arte representa al juego (en The Last of Us Part II el 4K es un pasillo oscuro
// sin personajes, y el primero de la lista es una escena reconocible), y los votos de la API son
// 0 en todos, así que no hay señal que explotar. Se respeta el orden que devuelve SteamGridDB.
async fn first_hero(client: &reqwest::Client, key: &str, game_id: u32) -> Option<String> {
    let url = format!("{API_BASE}/heroes/game/{game_id}?nsfw=false&humor=false");
    let resp = client.get(url).bearer_auth(key).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let parsed: HeroResp = resp.json().await.ok()?;
    parsed.data.into_iter().next().map(|a| a.url)
}

async fn first_icon(client: &reqwest::Client, key: &str, game_id: u32) -> Option<String> {
    let url = format!("{API_BASE}/icons/game/{game_id}?nsfw=false&humor=false");
    let resp = client.get(url).bearer_auth(key).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let parsed: HeroResp = resp.json().await.ok()?;
    parsed.data.into_iter().next().map(|a| a.url)
}

// Respaldo cuando el juego no tiene icon en SteamGridDB: el grid cuadrado es arte a sangre y
// encaja en un hueco cuadrado, aunque no sea un icono propiamente dicho.
async fn first_square(client: &reqwest::Client, key: &str, game_id: u32) -> Option<String> {
    let url =
        format!("{API_BASE}/grids/game/{game_id}?dimensions=512x512&types=static&nsfw=false&humor=false");
    let resp = client.get(url).bearer_auth(key).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let parsed: HeroResp = resp.json().await.ok()?;
    parsed.data.into_iter().next().map(|a| a.url)
}

async fn best_art(client: &reqwest::Client, key: &str, game_id: u32) -> Option<String> {
    match first_icon(client, key, game_id).await {
        Some(url) => Some(url),
        None => first_square(client, key, game_id).await,
    }
}

pub async fn game_icon(
    app: &tauri::AppHandle,
    name: &str,
    steam_appid: Option<u32>,
) -> Option<String> {
    let dir = app.path().app_cache_dir().ok()?.join("artwork");
    let _ = std::fs::create_dir_all(&dir);
    // El prefijo acompaña al orden de fuentes: al cambiarlo, el arte ya cacheado con el orden
    // anterior deja de usarse en vez de quedarse pegado para siempre.
    let cache_key = match steam_appid {
        Some(id) => format!("art-steam-{id}"),
        None => format!("art-{}", slug(name)),
    };
    let path = dir.join(&cache_key);

    if let Ok(bytes) = std::fs::read(&path) {
        if !bytes.is_empty() {
            return Some(to_data_url(&bytes));
        }
    }

    let client = reqwest::Client::new();
    // Discord primero: su lista detectable ya está cacheada para el detector de juegos, así que
    // sale gratis, y su arte es homogéneo entre juegos. SteamGridDB queda de respaldo.
    let list_art = crate::detect::art_for(app, name).await;
    if let Some(url) = list_art.as_ref().and_then(|a| a.icon_url.as_deref()) {
        // 256 px: el original del CDN puede ser de 1024 y esto viaja al frontend como data URL
        // para verse a 30. El Rich Presence sigue usando la URL sin recortar.
        if let Some(bytes) = download(&client, &format!("{url}?size=256")).await {
            if !bytes.is_empty() {
                let _ = std::fs::write(&path, &bytes);
                return Some(to_data_url(&bytes));
            }
        }
    }
    // El AppID de la lista también sirve aquí: con él el respaldo va por búsqueda exacta en vez
    // de por nombre, que es lo que confunde remasters y secuelas.
    let appid = steam_appid.or_else(|| list_art.and_then(|a| a.steam_appid));
    let bytes = match appid {
        Some(id) => steam_icon(&client, app, id).await?,
        None => name_icon(&client, app, name).await?,
    };
    if bytes.is_empty() {
        return None;
    }
    let _ = std::fs::write(&path, &bytes);
    Some(to_data_url(&bytes))
}

async fn steam_icon(client: &reqwest::Client, app: &tauri::AppHandle, appid: u32) -> Option<Vec<u8>> {
    // SteamGridDB primero si hay API key: iconos cuadrados reales.
    if let Some(key) = api_key(app) {
        if let Some(game_id) = sgdb_by_steam(client, &key, appid).await {
            if let Some(art_url) = best_art(client, &key, game_id).await {
                if let Some(bytes) = download(client, &art_url).await {
                    return Some(bytes);
                }
            }
        }
    }
    // Fallback sin API key: portada vertical del CDN de Steam (600×900), se recorta a cuadrado en CSS.
    let cdn_url = format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{appid}/library_600x900.jpg");
    download(client, &cdn_url).await
}

async fn name_icon(client: &reqwest::Client, app: &tauri::AppHandle, name: &str) -> Option<Vec<u8>> {
    let key = api_key(app)?;
    let game_id = search_game(client, &key, name).await?;
    let art_url = best_art(client, &key, game_id).await?;
    download(client, &art_url).await
}
