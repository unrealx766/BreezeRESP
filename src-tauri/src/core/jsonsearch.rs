//! RedisJSON & RediSearch data access (module-backed features).
//!
//! All operations require the corresponding Redis module; callers must check
//! `crate::core::capability` first so users get a friendly guidance message
//! instead of an opaque "unknown command" error.

use redis::AsyncCommands;
use serde::{Deserialize, Serialize};

/// One field definition of a RediSearch index (parsed from FT.INFO).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FtField {
    pub identifier: String,
    pub attribute: String,
    /// TEXT / TAG / NUMERIC / GEO / VECTOR / ...
    pub field_type: String,
    // Vector parameters (VECTOR fields only)
    pub vector_algorithm: Option<String>,
    pub vector_dim: Option<u64>,
    pub vector_distance_metric: Option<String>,
    pub vector_data_type: Option<String>,
}

/// Parsed FT.INFO summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FtIndexInfo {
    pub name: String,
    pub num_docs: u64,
    pub fields: Vec<FtField>,
    /// Raw index definition options not modelled above (e.g. prefix list).
    pub prefixes: Vec<String>,
}

/// A document returned by FT.SEARCH.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FtDocument {
    pub id: String,
    pub score: Option<f64>,
    pub fields: Vec<(String, String)>,
}

/// FT.SEARCH result set.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FtSearchResult {
    pub total: u64,
    pub docs: Vec<FtDocument>,
    /// Elapsed time in milliseconds reported by the server (first array item
    /// after the total in newer versions is still total; kept for future use).
    pub elapsed_ms: Option<f64>,
}

/// Field specification used to build an FT.CREATE command.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FtFieldSpec {
    pub identifier: String,
    /// Optional alias (AS attribute). Defaults to identifier when empty.
    pub attribute: Option<String>,
    /// TEXT / TAG / NUMERIC / GEO / VECTOR
    pub field_type: String,
    /// TAG separator (defaults to ",").
    pub separator: Option<String>,
    // Vector parameters (required when field_type == VECTOR)
    pub vector_algorithm: Option<String>,
    pub vector_dim: Option<u64>,
    pub vector_distance_metric: Option<String>,
    pub vector_data_type: Option<String>,
}

/// Index creation specification for FT.CREATE.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FtCreateSpec {
    pub name: String,
    /// Source type: HASH (default) or JSON.
    pub on_type: Option<String>,
    pub prefixes: Vec<String>,
    pub fields: Vec<FtFieldSpec>,
}

fn flat_pairs(raw: &redis::Value) -> Vec<(String, redis::Value)> {
    let items = match raw {
        redis::Value::Array(items) => items,
        _ => return Vec::new(),
    };
    items
        .chunks(2)
        .filter(|c| c.len() == 2)
        .filter_map(|c| {
            redis::from_redis_value::<String>(&c[0])
                .ok()
                .map(|k| (k, c[1].clone()))
        })
        .collect()
}

/// Parse one FT.INFO field definition (flat `[identifier, x, attribute, y, type, z, ...]`).
fn parse_ft_field(raw: &redis::Value) -> Option<FtField> {
    let pairs = flat_pairs(raw);
    let get = |name: &str| pairs.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v);
    let get_str = |name: &str| get(name).and_then(|v| redis::from_redis_value::<String>(v).ok());

    let identifier = get_str("identifier")?;
    let attribute = get_str("attribute").unwrap_or_else(|| identifier.clone());
    let field_type = get_str("type").unwrap_or_default();

    // VECTOR fields carry their parameters as a flat list under the same
    // definition array: [..., "VECTOR", <algorithm>, <nargs>, k, v, k, v, ...].
    let mut vector_algorithm = None;
    let mut vector_dim = None;
    let mut vector_distance_metric = None;
    let mut vector_data_type = None;
    if field_type.eq_ignore_ascii_case("VECTOR") {
        let items = match raw {
            redis::Value::Array(items) => items,
            _ => return None,
        };
        // Locate the "VECTOR" token; everything after it is algorithm + flat kv params.
        let pos = items.iter().position(|v| {
            redis::from_redis_value::<String>(v)
                .map(|s| s.eq_ignore_ascii_case("VECTOR"))
                .unwrap_or(false)
        })?;
        let rest = &items[pos + 1..];
        if let Some(algo) = rest.first() {
            vector_algorithm = redis::from_redis_value::<String>(algo).ok();
        }
        // Skip nargs (rest[1]); parse flat kv pairs from rest[2..].
        let kv = &rest[rest.len().min(2)..];
        for pair in kv.chunks(2) {
            if pair.len() < 2 {
                continue;
            }
            let (Ok(k), Ok(v)) = (
                redis::from_redis_value::<String>(&pair[0]),
                redis::from_redis_value::<String>(&pair[1]),
            ) else {
                continue;
            };
            match k.to_uppercase().as_str() {
                "DIM" => vector_dim = v.parse().ok(),
                "DISTANCE_METRIC" => vector_distance_metric = Some(v),
                "TYPE" => vector_data_type = Some(v),
                _ => {}
            }
        }
    }

    Some(FtField {
        identifier,
        attribute,
        field_type,
        vector_algorithm,
        vector_dim,
        vector_distance_metric,
        vector_data_type,
    })
}

/// Collects RedisJSON & RediSearch data.
pub struct JsonSearchCollector;

impl JsonSearchCollector {
    pub fn new() -> Self {
        Self
    }

    // ── RedisJSON ──

    /// JSON.GET key [path]; returns the raw JSON string.
    pub async fn json_get(
        &self,
        conn: &mut impl AsyncCommands,
        key: &str,
        path: &str,
    ) -> Result<String, String> {
        let mut cmd = redis::cmd("JSON.GET");
        cmd.arg(key);
        if !path.is_empty() {
            cmd.arg(path);
        }
        let raw: Option<String> = cmd
            .query_async(conn)
            .await
            .map_err(|e| format!("JSON.GET error: {}", e))?;
        raw.ok_or_else(|| "Key or path not found".to_string())
    }

    /// JSON.SET key path value.
    pub async fn json_set(
        &self,
        conn: &mut impl AsyncCommands,
        key: &str,
        path: &str,
        value: &str,
    ) -> Result<(), String> {
        redis::cmd("JSON.SET")
            .arg(key)
            .arg(if path.is_empty() { "$" } else { path })
            .arg(value)
            .query_async(conn)
            .await
            .map_err(|e| format!("JSON.SET error: {}", e))
    }

    /// JSON.DEL key [path]; returns removed count.
    pub async fn json_del(
        &self,
        conn: &mut impl AsyncCommands,
        key: &str,
        path: &str,
    ) -> Result<i64, String> {
        let mut cmd = redis::cmd("JSON.DEL");
        cmd.arg(key);
        if !path.is_empty() {
            cmd.arg(path);
        }
        cmd.query_async(conn)
            .await
            .map_err(|e| format!("JSON.DEL error: {}", e))
    }

    /// JSON.TYPE key [path].
    pub async fn json_type(
        &self,
        conn: &mut impl AsyncCommands,
        key: &str,
        path: &str,
    ) -> Result<String, String> {
        let mut cmd = redis::cmd("JSON.TYPE");
        cmd.arg(key);
        if !path.is_empty() {
            cmd.arg(path);
        }
        cmd.query_async(conn)
            .await
            .map_err(|e| format!("JSON.TYPE error: {}", e))
    }

    // ── RediSearch ──

    /// FT._LIST → index names.
    pub async fn ft_list(
        &self,
        conn: &mut impl AsyncCommands,
    ) -> Result<Vec<String>, String> {
        redis::cmd("FT._LIST")
            .query_async(conn)
            .await
            .map_err(|e| format!("FT._LIST error: {}", e))
    }

    /// FT.INFO index → parsed summary.
    pub async fn ft_info(
        &self,
        conn: &mut impl AsyncCommands,
        index: &str,
    ) -> Result<FtIndexInfo, String> {
        let raw: redis::Value = redis::cmd("FT.INFO")
            .arg(index)
            .query_async(conn)
            .await
            .map_err(|e| format!("FT.INFO error: {}", e))?;

        let pairs = flat_pairs(&raw);
        let get = |name: &str| pairs.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v);

        let num_docs = get("num_docs")
            .and_then(|v| redis::from_redis_value::<u64>(v).ok())
            .unwrap_or(0);

        let fields = get("fields")
            .map(|v| match v {
                redis::Value::Array(items) => items.iter().filter_map(parse_ft_field).collect(),
                _ => Vec::new(),
            })
            .unwrap_or_default();

        // `index_definition` is a flat array containing PREFIX entries.
        let prefixes = get("index_definition")
            .map(|v| {
                let items = flat_pairs(v);
                items
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case("prefixes"))
                    .and_then(|(_, v)| redis::from_redis_value::<Vec<String>>(v).ok())
                    .unwrap_or_default()
            })
            .unwrap_or_default();

        Ok(FtIndexInfo {
            name: index.to_string(),
            num_docs,
            fields,
            prefixes,
        })
    }

    /// FT.SEARCH with optional LIMIT and PARAMS (for KNN vector queries).
    /// Param values are raw byte vectors: binary payloads (e.g. FLOAT32
    /// blobs) travel from the frontend as number arrays and must be sent
    /// to the server as bytes, never reinterpreted as UTF-8 text.
    pub async fn ft_search(
        &self,
        conn: &mut impl AsyncCommands,
        index: &str,
        query: &str,
        offset: u64,
        limit: u64,
        params: &[(String, Vec<u8>)],
        with_scores: bool,
    ) -> Result<FtSearchResult, String> {
        let mut cmd = redis::cmd("FT.SEARCH");
        cmd.arg(index).arg(query).arg("LIMIT").arg(offset).arg(limit);
        if with_scores {
            cmd.arg("WITHSCORES");
        }
        if !params.is_empty() {
            cmd.arg("PARAMS").arg(params.len() * 2);
            for (k, v) in params {
                cmd.arg(k).arg(v.as_slice());
            }
        }
        let raw: redis::Value = cmd
            .query_async(conn)
            .await
            .map_err(|e| format!("FT.SEARCH error: {}", e))?;

        let items = match raw {
            redis::Value::Array(items) => items,
            _ => return Err("Unexpected FT.SEARCH response".to_string()),
        };
        if items.is_empty() {
            return Ok(FtSearchResult {
                total: 0,
                docs: Vec::new(),
                elapsed_ms: None,
            });
        }
        let total: u64 = redis::from_redis_value(&items[0]).unwrap_or(0);

        // Each doc: [id, (score?), [field, value, ...]] — score present only with WITHSCORES.
        let step = if with_scores { 3 } else { 2 };
        let mut docs = Vec::new();
        let mut i = 1;
        while i + step <= items.len() {
            let slice = &items[i..i + step];
            i += step;
            let id: String = match redis::from_redis_value(&slice[0]) {
                Ok(id) => id,
                Err(_) => continue,
            };
            let score = if with_scores && slice.len() >= 3 {
                redis::from_redis_value::<f64>(&slice[1]).ok()
            } else {
                None
            };
            let fields_val = if with_scores { slice.get(2) } else { slice.get(1) };
            let fields: Vec<(String, String)> = fields_val
                .and_then(|v| redis::from_redis_value::<Vec<String>>(v).ok())
                .map(|flat| {
                    flat.chunks(2)
                        .filter(|c| c.len() == 2)
                        .map(|c| (c[0].clone(), c[1].clone()))
                        .collect()
                })
                .unwrap_or_default();
            docs.push(FtDocument { id, score, fields });
        }

        Ok(FtSearchResult {
            total,
            docs,
            elapsed_ms: None,
        })
    }

    /// Build and execute FT.CREATE from a structured spec.
    pub async fn ft_create(
        &self,
        conn: &mut impl AsyncCommands,
        spec: &FtCreateSpec,
    ) -> Result<(), String> {
        let mut cmd = redis::cmd("FT.CREATE");
        cmd.arg(&spec.name);
        if let Some(on) = &spec.on_type {
            let on = on.to_uppercase();
            if on == "HASH" || on == "JSON" {
                cmd.arg("ON").arg(&on);
            }
        }
        if !spec.prefixes.is_empty() {
            cmd.arg("PREFIX").arg(spec.prefixes.len());
            for p in &spec.prefixes {
                cmd.arg(p);
            }
        }
        cmd.arg("SCHEMA");
        for f in &spec.fields {
            cmd.arg(&f.identifier);
            if let Some(attr) = &f.attribute {
                if !attr.is_empty() && attr != &f.identifier {
                    cmd.arg("AS").arg(attr);
                }
            }
            let ft = f.field_type.to_uppercase();
            cmd.arg(&ft);
            match ft.as_str() {
                "TAG" => {
                    if let Some(sep) = &f.separator {
                        if !sep.is_empty() {
                            cmd.arg("SEPARATOR").arg(sep);
                        }
                    }
                }
                "VECTOR" => {
                    let algo = f
                        .vector_algorithm
                        .clone()
                        .unwrap_or_else(|| "FLAT".to_string());
                    let dim = f
                        .vector_dim
                        .ok_or_else(|| "Vector field requires a dimension".to_string())?;
                    let dtype = f
                        .vector_data_type
                        .clone()
                        .unwrap_or_else(|| "FLOAT32".to_string());
                    let metric = f
                        .vector_distance_metric
                        .clone()
                        .unwrap_or_else(|| "COSINE".to_string());
                    // TYPE dtype DIM dim DISTANCE_METRIC metric → 6 args
                    cmd.arg(&algo).arg(6usize);
                    cmd.arg("TYPE").arg(&dtype);
                    cmd.arg("DIM").arg(dim);
                    cmd.arg("DISTANCE_METRIC").arg(&metric);
                }
                _ => {}
            }
        }
        cmd.query_async(conn)
            .await
            .map_err(|e| format!("FT.CREATE error: {}", e))
    }

    /// FT.DROPINDEX index [DD].
    pub async fn ft_drop(
        &self,
        conn: &mut impl AsyncCommands,
        index: &str,
        delete_docs: bool,
    ) -> Result<(), String> {
        let mut cmd = redis::cmd("FT.DROPINDEX");
        cmd.arg(index);
        if delete_docs {
            cmd.arg("DD");
        }
        cmd.query_async(conn)
            .await
            .map_err(|e| format!("FT.DROPINDEX error: {}", e))
    }
}

impl Default for JsonSearchCollector {
    fn default() -> Self {
        Self::new()
    }
}
