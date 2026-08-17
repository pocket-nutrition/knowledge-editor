//! pocket-nutrition-review MCP server — stdio JSON-RPC 2.0 transport.
//!
//! Exposes all knowledge-editor operations as MCP tools, plus review-specific
//! additions: list_review_queue, get_review_progress, mark_reviewed,
//! get_dry_run_sample, and clone_ingredient.
//!
//! State is loaded from the pocket-nutrition/knowledge repo on startup (same
//! Repo::discover() logic as the Tauri app). Review progress is persisted to
//! $KNOWLEDGE_REPO/.review-progress.json between sessions.
//! Mutation history is appended to $KNOWLEDGE_REPO/.review-history.jsonl.
//!
//! Actor model:
//!   actor="claude" → confidence ≤ 0.9, match_method="ai_review"  (default)
//!   actor="human"  → confidence = 1.0, match_method="manual"
//!
//! Build:  cargo build --bin review-mcp   (from src-tauri/)
//! Run:    POCKET_NUTRITION_KNOWLEDGE_PATH=/path/to/repo ./review-mcp

use std::collections::HashMap;
use std::collections::HashSet;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::{Mutex, RwLock};
use serde_json::{json, Value};

use pocket_nutrition_knowledge_editor_lib::{
    error::AppResult,
    indexer::Index,
    model::{
        FoodGroup, Ingredient, ServingMode, Source, SourceMapping, SourceMappingCookingMethod,
        TypicalState,
    },
    repo::Repo,
    schema::Schemas,
    yaml_io::write_yaml_atomic,
};

// ── State ────────────────────────────────────────────────────────────────────

struct AppState {
    repo: Repo,
    schemas: Schemas,
    index: Arc<RwLock<Index>>,
    progress: Arc<Mutex<ReviewProgress>>,
    session_fork_count: Arc<std::sync::atomic::AtomicU32>,
    history_path: std::path::PathBuf,
    /// Culinary unit names accepted by `serving_unit_types`, loaded from
    /// `serving_unit_types.yaml`. Names outside this set are silently dropped
    /// by the downstream resolvers (pocket-nutrition/api / pocket-nutrition/community), so the
    /// setter rejects them rather than writing data that vanishes.
    known_units: Arc<HashSet<String>>,
}

const MAX_FORKS_PER_SESSION: u32 = 20;

/// Fallback unit set if `serving_unit_types.yaml` is missing or unreadable.
/// Kept in sync with that file; the file is the source of truth at runtime.
const FALLBACK_SERVING_UNITS: &[&str] = &[
    "pinch",
    "teaspoon",
    "tablespoon",
    "cup",
    "handful",
    "small_handful",
    "glass",
];

/// Read the known culinary unit names from `serving_unit_types.yaml` (keys of a
/// `name: volume_ml` mapping). Falls back to [`FALLBACK_SERVING_UNITS`] if the
/// file is absent, empty, or unparseable.
fn load_known_units(repo_root: &std::path::Path) -> HashSet<String> {
    let path = repo_root.join("serving_unit_types.yaml");
    let parsed: Option<std::collections::BTreeMap<String, f64>> = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_yaml::from_str(&s).ok());
    match parsed {
        Some(map) if !map.is_empty() => map.into_keys().collect(),
        _ => FALLBACK_SERVING_UNITS
            .iter()
            .map(|s| s.to_string())
            .collect(),
    }
}

/// Validate (and dedup, preserving order) a list of serving unit names against
/// the known set. Returns an error naming the valid units if any entry is
/// unknown — unknown units must be proposed for human wiring, never written.
fn validate_serving_units(
    units: &[String],
    known: &HashSet<String>,
) -> Result<Vec<String>, String> {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut out: Vec<String> = Vec::with_capacity(units.len());
    for u in units {
        if !known.contains(u) {
            let mut valid: Vec<&str> = known.iter().map(String::as_str).collect();
            valid.sort_unstable();
            return Err(format!(
                "unknown serving unit '{u}'. Known units: {}. To add a new unit, record a \
                 PROPOSITION UNITÉ and have a human wire it into serving_unit_types.yaml plus the \
                 downstream maps — do not assign it here (it would be silently dropped).",
                valid.join(", ")
            ));
        }
        if seen.insert(u.as_str()) {
            out.push(u.clone());
        }
    }
    Ok(out)
}

// ── ReviewProgress ────────────────────────────────────────────────────────────

struct ReviewProgress {
    human_reviewed_ids: HashSet<String>,
    ai_reviewed_ids: HashSet<String>,
    path: PathBuf,
}

impl ReviewProgress {
    fn load(repo_root: &std::path::Path) -> Self {
        let path = repo_root.join(".review-progress.json");
        let val: Value = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();

        let parse_ids = |key: &str| -> HashSet<String> {
            val[key]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default()
        };

        let human_reviewed_ids = if val["human_reviewed_ids"].is_array() {
            parse_ids("human_reviewed_ids")
        } else {
            // backward-compat: migrate old flat "reviewed_ids" → human_reviewed_ids
            parse_ids("reviewed_ids")
        };
        let ai_reviewed_ids = parse_ids("ai_reviewed_ids");

        Self {
            human_reviewed_ids,
            ai_reviewed_ids,
            path,
        }
    }

    fn save(&self) {
        let mut human: Vec<&str> = self.human_reviewed_ids.iter().map(String::as_str).collect();
        human.sort();
        let mut ai: Vec<&str> = self.ai_reviewed_ids.iter().map(String::as_str).collect();
        ai.sort();
        let val = json!({
            "human_reviewed_ids": human,
            "ai_reviewed_ids": ai,
            "updated_at": simple_now()
        });
        if let Ok(s) = serde_json::to_string_pretty(&val) {
            let _ = std::fs::write(&self.path, s);
        }
    }

    fn mark(&mut self, id: &str, actor: &str) {
        if actor == "human" {
            self.human_reviewed_ids.insert(id.to_string());
            self.ai_reviewed_ids.remove(id); // human review supersedes AI
        } else if !self.human_reviewed_ids.contains(id) {
            self.ai_reviewed_ids.insert(id.to_string());
        }
        self.save();
    }

    /// Returns "human", "ai", or None.
    fn reviewed_by(&self, id: &str) -> Option<&'static str> {
        if self.human_reviewed_ids.contains(id) {
            Some("human")
        } else if self.ai_reviewed_ids.contains(id) {
            Some("ai")
        } else {
            None
        }
    }

    fn human_count(&self) -> usize {
        self.human_reviewed_ids.len()
    }
    fn ai_count(&self) -> usize {
        self.ai_reviewed_ids.len()
    }
    fn total_reviewed(&self) -> usize {
        self.human_reviewed_ids.len() + self.ai_reviewed_ids.len()
    }
}

// ── Audit history ─────────────────────────────────────────────────────────────

fn append_history(path: &std::path::Path, entry: Value) {
    if let Ok(s) = serde_json::to_string(&entry) {
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(f, "{s}");
        }
    }
}

fn simple_now() -> String {
    let dur = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs();
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let (y, mo, d) = days_to_ymd(secs / 86400);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    let mut y = 1970u64;
    loop {
        let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
        let yd = if leap { 366 } else { 365 };
        if days < yd {
            break;
        }
        days -= yd;
        y += 1;
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let mds: &[u64] = if leap {
        &[31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        &[31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut mo = 1u64;
    for &md in mds {
        if days < md {
            break;
        }
        days -= md;
        mo += 1;
    }
    (y, mo, days + 1)
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .compact()
        .init();

    let state = match build_state() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("review-mcp: startup failed: {e}");
            std::process::exit(1);
        }
    };

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let req: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let id = req.get("id").cloned();
        let method = req["method"].as_str().unwrap_or("");

        // Notifications have no id — skip silently
        if id.is_none() {
            continue;
        }

        let empty = json!({});
        let params = req.get("params").unwrap_or(&empty);
        let response = match dispatch(&state, method, params) {
            Ok(r) => json!({"jsonrpc":"2.0","id":id,"result":r}),
            Err(e) => json!({"jsonrpc":"2.0","id":id,"error":{"code":-32000,"message":e}}),
        };
        if let Ok(s) = serde_json::to_string(&response) {
            let _ = writeln!(out, "{s}");
            let _ = out.flush();
        }
    }
}

fn build_state() -> AppResult<AppState> {
    let repo = Repo::discover()?;
    tracing::info!(root = %repo.root.display(), "loaded knowledge repo");
    let schemas = Schemas::load(&repo.schemas_dir())?;
    let index = Index::build(&repo)?;
    tracing::info!(
        ingredients = index.ingredients.len(),
        mappings = index.mappings.len(),
        "index built"
    );
    let history_path = repo.root.join(".review-history.jsonl");
    let progress = ReviewProgress::load(&repo.root);
    let known_units = Arc::new(load_known_units(&repo.root));
    tracing::info!(units = known_units.len(), "loaded serving unit types");
    Ok(AppState {
        repo,
        schemas,
        index: Arc::new(RwLock::new(index)),
        progress: Arc::new(Mutex::new(progress)),
        session_fork_count: Arc::new(std::sync::atomic::AtomicU32::new(0)),
        history_path,
        known_units,
    })
}

// ── MCP dispatch ──────────────────────────────────────────────────────────────

fn dispatch(state: &AppState, method: &str, params: &Value) -> Result<Value, String> {
    match method {
        "initialize" => Ok(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {"tools": {"listChanged": false}},
            "serverInfo": {"name": "pocket-nutrition-review", "version": "0.3.0"}
        })),

        "tools/list" => Ok(json!({"tools": tool_definitions()})),

        "tools/call" => {
            let name = params["name"].as_str().unwrap_or("");
            let empty = json!({});
            let args = params.get("arguments").unwrap_or(&empty);
            call_tool(state, name, args)
                .map(|v| json!({"content":[{"type":"text","text":v.to_string()}],"isError":false}))
        }

        _ => Err(format!("unknown method: {method}")),
    }
}

// ── Tool definitions ─────────────────────────────────────────────────────────

fn t(name: &str, desc: &str, schema: Value) -> Value {
    json!({"name":name,"description":desc,"inputSchema":schema})
}

fn tool_definitions() -> Vec<Value> {
    vec![
        // Review queue
        t("list_review_queue","List ingredients sorted by mapping count desc. Unreviewed first.",
          json!({"type":"object","properties":{"offset":{"type":"integer","default":0},"limit":{"type":"integer","default":20}}})),
        t("get_review_progress","Return {total, reviewed_count, remaining}.",
          json!({"type":"object","properties":{}})),
        t("mark_reviewed","Mark an ingredient as reviewed. actor='claude' (default, ai_reviewed_ids) or 'human' (human_reviewed_ids, supersedes AI).",
          json!({"type":"object","required":["ingredient_id"],"properties":{"ingredient_id":{"type":"string"},"actor":{"type":"string","enum":["claude","human"],"default":"claude"}}})),
        t("get_dry_run_sample","Return a representative sample: per_group ingredients per food_group from the most-mapped. Use to calibrate the review prompt before the full run.",
          json!({"type":"object","properties":{"per_group":{"type":"integer","default":2,"description":"1-5 ingredients per food group"}}})),

        // Ingredient read
        t("get_ingredient","Get full ingredient + linked mappings + cooking relationships.",
          json!({"type":"object","required":["id"],"properties":{"id":{"type":"string"}}})),
        t("search_ingredients","Full-text search on name_en, name_fr, id for reassignment target lookup.",
          json!({"type":"object","required":["query"],"properties":{"query":{"type":"string"},"limit":{"type":"integer","default":10}}})),
        t("list_ingredients","List with filters. sort options: name | mapping_count_desc | score_desc | unvalidated_desc.",
          json!({"type":"object","properties":{"search":{"type":"string"},"food_group":{"type":"string"},"sort":{"type":"string"},"orphans_filter":{"type":"string","enum":["hide","only","all"]},"offset":{"type":"integer","default":0},"limit":{"type":"integer","default":50}}})),

        // Ingredient write
        t("save_ingredient","Full ingredient save. Prefer set_* tools for single-field updates.",
          json!({"type":"object","required":["ingredient"],"properties":{"ingredient":{"type":"object"},"dry_run":{"type":"boolean","default":false}}})),
        t("clone_ingredient","Fork a new ingredient from an existing one (copies states, can_eat_*, display_unit, serving_mode). Refuses to overwrite existing id. Capped at 20 forks/session.",
          json!({"type":"object","required":["source_id","new_id","new_name_en"],"properties":{"source_id":{"type":"string"},"new_id":{"type":"string","description":"snake_case max 40 chars"},"new_name_en":{"type":"string"},"new_name_fr":{"type":"string"},"dry_run":{"type":"boolean","default":false}}})),
        t("delete_ingredient","Permanently delete an ingredient YAML (git tracks history).",
          json!({"type":"object","required":["id"],"properties":{"id":{"type":"string"},"dry_run":{"type":"boolean","default":false}}})),

        // Ingredient field setters (all support dry_run)
        t("set_ingredient_can_eat_raw","Set can_eat_raw. Pass value: null to clear.",
          json!({"type":"object","required":["id"],"properties":{"id":{"type":"string"},"value":{"type":["boolean","null"]},"dry_run":{"type":"boolean","default":false}}})),
        t("set_ingredient_can_eat_cooked","Set can_eat_cooked. Pass value: null to clear.",
          json!({"type":"object","required":["id"],"properties":{"id":{"type":"string"},"value":{"type":["boolean","null"]},"dry_run":{"type":"boolean","default":false}}})),
        t("set_ingredient_display_unit","Set display_unit to 'g', 'ml', or null to clear.",
          json!({"type":"object","required":["id"],"properties":{"id":{"type":"string"},"display_unit":{"type":["string","null"],"enum":["g","ml",null]},"dry_run":{"type":"boolean","default":false}}})),
        t("set_ingredient_serving_mode","Set serving_mode (item/weight/volume/measure) or null to clear.",
          json!({"type":"object","required":["id"],"properties":{"id":{"type":"string"},"serving_mode":{"type":["string","null"],"enum":["item","weight","volume","measure",null]},"dry_run":{"type":"boolean","default":false}}})),
        t("set_ingredient_typical_state","Set typical_state (raw/cooked/processed/boiled/steamed/grilled/roasted/fried) or null.",
          json!({"type":"object","required":["id"],"properties":{"id":{"type":"string"},"typical_state":{"type":["string","null"]},"dry_run":{"type":"boolean","default":false}}})),
        t("set_ingredient_food_group","Set food_group enum.",
          json!({"type":"object","required":["id","food_group"],"properties":{"id":{"type":"string"},"food_group":{"type":"string"},"dry_run":{"type":"boolean","default":false}}})),
        t("set_ingredient_state_portion","Set portion_g for raw/cooked/processed state. null to clear.",
          json!({"type":"object","required":["id","state_key"],"properties":{"id":{"type":"string"},"state_key":{"type":"string","enum":["raw","cooked","processed"]},"portion_g":{"type":["number","null"]},"dry_run":{"type":"boolean","default":false}}})),
        t("set_ingredient_state_density","Set density_g_ml for a state. null to clear.",
          json!({"type":"object","required":["id","state_key"],"properties":{"id":{"type":"string"},"state_key":{"type":"string","enum":["raw","cooked","processed"]},"density_g_ml":{"type":["number","null"]},"dry_run":{"type":"boolean","default":false}}})),
        t("set_ingredient_serving_unit_types","Set serving_unit_types — the culinary measures offered in the UI for non-countable foods (pinch, teaspoon, tablespoon, cup, handful, small_handful, glass). Only units known to serving_unit_types.yaml are accepted; unknown names are rejected — propose them for human wiring instead. Resolves to grams via the ingredient's density. Pass [] or null to clear.",
          json!({"type":"object","required":["id"],"properties":{"id":{"type":"string"},"serving_unit_types":{"oneOf":[{"type":"array","items":{"type":"string"}},{"type":"null"}]},"dry_run":{"type":"boolean","default":false}}})),

        // Mapping tools (all support dry_run)
        t("set_mapping_cooking_method","Set cooking_method on a source mapping.",
          json!({"type":"object","required":["source","source_id","cooking_method"],"properties":{"source":{"type":"string"},"source_id":{"type":"string"},"cooking_method":{"type":"string","enum":["raw","boiled","steamed","grilled","roasted","fried","cooked","dry"]},"dry_run":{"type":"boolean","default":false}}})),
        t("link_mapping","Link (or reassign) a mapping. actor='claude' (default): confidence=0.9, match_method=ai_review. actor='human': confidence=1.0, match_method=manual.",
          json!({"type":"object","required":["source","source_id","ingredient_id"],"properties":{"source":{"type":"string"},"source_id":{"type":"string"},"ingredient_id":{"type":"string"},"actor":{"type":"string","enum":["claude","human"],"default":"claude"},"dry_run":{"type":"boolean","default":false}}})),
        t("unlink_mapping","Clear canonical_ingredient_id — mapping re-enters the unlinked pool.",
          json!({"type":"object","required":["source","source_id"],"properties":{"source":{"type":"string"},"source_id":{"type":"string"},"dry_run":{"type":"boolean","default":false}}})),
        t("mark_mapping_composite","Mark mapping as intentionally un-mappable (multi-ingredient dish).",
          json!({"type":"object","required":["source","source_id"],"properties":{"source":{"type":"string"},"source_id":{"type":"string"},"dry_run":{"type":"boolean","default":false}}})),
        t("validate_mapping","Promote link to human-validated (match_method=manual, confidence=1.0).",
          json!({"type":"object","required":["source","source_id"],"properties":{"source":{"type":"string"},"source_id":{"type":"string"},"dry_run":{"type":"boolean","default":false}}})),
        t("search_unlinked_mappings","Search unlinked mappings by label or source_id.",
          json!({"type":"object","required":["query"],"properties":{"source":{"type":"string"},"query":{"type":"string"},"limit":{"type":"integer","default":20}}})),

        // Translation / aliases (AI-fillable fields)
        t("set_ingredient_name_fr","Set or clear the French name. Pass null to clear.",
          json!({"type":"object","required":["id"],"properties":{"id":{"type":"string"},"name_fr":{"type":["string","null"]},"dry_run":{"type":"boolean","default":false}}})),
        t("set_ingredient_name_en","Set the canonical English name (required field, cannot be empty or null). Use to fix English-only/capitalized seed names, e.g. 'Bigorneau' -> 'periwinkle'.",
          json!({"type":"object","required":["id","name_en"],"properties":{"id":{"type":"string"},"name_en":{"type":"string","minLength":1},"dry_run":{"type":"boolean","default":false}}})),
        t("set_ingredient_aliases","Set aliases_en or aliases_fr. Pass aliases: null to clear.",
          json!({"type":"object","required":["id","lang","aliases"],"properties":{"id":{"type":"string"},"lang":{"type":"string","enum":["en","fr"]},"aliases":{"oneOf":[{"type":"array","items":{"type":"string"}},{"type":"null"}]},"dry_run":{"type":"boolean","default":false}}})),
        t("set_ingredient_plural","Set name_en_plural or name_fr_plural. Pass plural: null to clear.",
          json!({"type":"object","required":["id","lang"],"properties":{"id":{"type":"string"},"lang":{"type":"string","enum":["en","fr"]},"plural":{"type":["string","null"]},"dry_run":{"type":"boolean","default":false}}})),
        t("set_ingredient_unit_label","Set unit_label_en or unit_label_fr — the label for ONE unit when serving_mode='item' (e.g. 'slice'/'tranche', 'clove'/'gousse'). Required when serving_mode=item. Pass unit_label: null to clear.",
          json!({"type":"object","required":["id","lang"],"properties":{"id":{"type":"string"},"lang":{"type":"string","enum":["en","fr"]},"unit_label":{"type":["string","null"]},"dry_run":{"type":"boolean","default":false}}})),

        // Bulk review application
        t("apply_review","Bulk-apply a full ingredient review in ONE call: patch ingredient fields, patch per-state portion/density, run mapping ops, and optionally mark reviewed. The ingredient is schema-validated and written ONCE; every field change still gets its own audit line in .review-history.jsonl. Prefer this over chains of set_ingredient_* calls. All inputs are validated before any write; mapping ops then apply in order (an op failure reports how many were applied).",
          json!({"type":"object","required":["ingredient_id"],"properties":{
            "ingredient_id":{"type":"string"},
            "fields":{"type":"object","description":"Ingredient field patches, key -> new value (null clears nullable fields). Allowed keys: name_en, name_fr, name_en_plural, name_fr_plural, unit_label_en, unit_label_fr, can_eat_raw, can_eat_cooked, display_unit ('g'|'ml'|null), serving_mode (item|weight|volume|measure|null), typical_state (raw|cooked|processed|boiled|steamed|grilled|roasted|fried|null), food_group, aliases_en, aliases_fr, serving_unit_types (validated against serving_unit_types.yaml)."},
            "states":{"type":"object","description":"Per-state patches: {\"raw\"|\"cooked\"|\"processed\": {portion_g?: number|null, density_g_ml?: number|null}}. Only the keys present are touched."},
            "mapping_ops":{"type":"array","description":"Mapping operations applied in order.","items":{"type":"object","required":["op","source","source_id"],"properties":{
              "op":{"type":"string","enum":["link","unlink","composite","set_cooking_method"]},
              "source":{"type":"string"},
              "source_id":{"type":"string"},
              "ingredient_id":{"type":"string","description":"link target (required for op=link)"},
              "cooking_method":{"type":"string","enum":["raw","boiled","steamed","grilled","roasted","fried","cooked","dry"],"description":"required for op=set_cooking_method"}}}},
            "mark_reviewed":{"type":"boolean","default":false,"description":"Mark the ingredient reviewed after all changes succeed."},
            "actor":{"type":"string","enum":["claude","human"],"default":"claude","description":"Used for link confidence/match_method and for mark_reviewed."},
            "dry_run":{"type":"boolean","default":false}}})),
    ]
}

// ── Tool dispatch ─────────────────────────────────────────────────────────────

fn call_tool(state: &AppState, name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "list_review_queue" => tool_list_review_queue(state, args),
        "get_review_progress" => tool_get_review_progress(state, args),
        "mark_reviewed" => tool_mark_reviewed(state, args),
        "get_dry_run_sample" => tool_get_dry_run_sample(state, args),
        "get_ingredient" => tool_get_ingredient(state, args),
        "search_ingredients" => tool_search_ingredients(state, args),
        "list_ingredients" => tool_list_ingredients(state, args),
        "save_ingredient" => tool_save_ingredient(state, args),
        "clone_ingredient" => tool_clone_ingredient(state, args),
        "delete_ingredient" => tool_delete_ingredient(state, args),
        "set_ingredient_can_eat_raw" => tool_set_ingredient_field(
            state,
            args,
            |i, v| {
                i.can_eat_raw = parse_opt_bool(v);
                Ok(())
            },
            "value",
        ),
        "set_ingredient_can_eat_cooked" => tool_set_ingredient_field(
            state,
            args,
            |i, v| {
                i.can_eat_cooked = parse_opt_bool(v);
                Ok(())
            },
            "value",
        ),
        "set_ingredient_display_unit" => tool_set_ingredient_display_unit(state, args),
        "set_ingredient_serving_mode" => tool_set_ingredient_serving_mode(state, args),
        "set_ingredient_typical_state" => tool_set_ingredient_typical_state(state, args),
        "set_ingredient_food_group" => tool_set_ingredient_food_group(state, args),
        "set_ingredient_state_portion" => tool_set_ingredient_state_portion(state, args),
        "set_ingredient_state_density" => tool_set_ingredient_state_density(state, args),
        "set_ingredient_serving_unit_types" => tool_set_ingredient_serving_unit_types(state, args),
        "set_mapping_cooking_method" => tool_set_mapping_cooking_method(state, args),
        "link_mapping" => tool_link_mapping(state, args),
        "unlink_mapping" => tool_unlink_mapping(state, args),
        "mark_mapping_composite" => tool_mark_mapping_composite(state, args),
        "validate_mapping" => tool_validate_mapping(state, args),
        "search_unlinked_mappings" => tool_search_unlinked_mappings(state, args),
        "set_ingredient_name_fr" => tool_set_ingredient_name_fr(state, args),
        "set_ingredient_name_en" => tool_set_ingredient_name_en(state, args),
        "set_ingredient_aliases" => tool_set_ingredient_aliases(state, args),
        "set_ingredient_plural" => tool_set_ingredient_plural(state, args),
        "set_ingredient_unit_label" => tool_set_ingredient_unit_label(state, args),
        "apply_review" => tool_apply_review(state, args),
        _ => Err(format!("unknown tool: {name}")),
    }
}

fn parse_opt_bool(v: &Value) -> Option<bool> {
    if v.is_null() {
        None
    } else {
        v.as_bool()
    }
}

// ── Shared index helpers ──────────────────────────────────────────────────────

fn mapping_counts(idx: &Index) -> HashMap<String, u32> {
    let mut counts: HashMap<String, u32> = HashMap::new();
    for m in idx.mappings.values() {
        if let Some(id) = &m.canonical_ingredient_id {
            if !id.is_empty() && id != SourceMapping::COMPOSITE_SENTINEL {
                *counts.entry(id.clone()).or_insert(0) += 1;
            }
        }
    }
    counts
}

fn ingredient_summary(i: &Ingredient, mapping_count: u32, reviewed_by: Option<&str>) -> Value {
    json!({
        "id": i.id,
        "name_en": i.name_en,
        "name_fr": i.name_fr,
        "food_group": serde_json::to_value(&i.food_group).unwrap_or(json!(null)),
        "mapping_count": mapping_count,
        "reviewed": reviewed_by.is_some(),
        "reviewed_by": reviewed_by,
        "can_eat_raw": i.can_eat_raw,
        "can_eat_cooked": i.can_eat_cooked,
        "display_unit": i.display_unit,
        "serving_mode": serde_json::to_value(&i.serving_mode).unwrap_or(json!(null)),
        "typical_state": serde_json::to_value(&i.typical_state).unwrap_or(json!(null)),
    })
}

// ── Review queue ──────────────────────────────────────────────────────────────

fn tool_list_review_queue(state: &AppState, args: &Value) -> Result<Value, String> {
    let offset = args["offset"].as_u64().unwrap_or(0) as usize;
    let limit = args["limit"].as_u64().unwrap_or(20) as usize;
    let idx = state.index.read();
    let progress = state.progress.lock();
    let counts = mapping_counts(&idx);

    let mut items: Vec<(&Ingredient, u32)> = idx
        .ingredients
        .values()
        .map(|i| (i, counts.get(&i.id).copied().unwrap_or(0)))
        .collect();

    // Sort: unreviewed first, then ai-reviewed, then human-reviewed; ties by mapping_count desc
    items.sort_by(|(a, ca), (b, cb)| {
        let rank = |id: &str| match progress.reviewed_by(id) {
            None => 0u8,
            Some("ai") => 1,
            Some("human") => 2,
            _ => 1,
        };
        rank(&a.id)
            .cmp(&rank(&b.id))
            .then_with(|| cb.cmp(ca))
            .then_with(|| a.name_en.to_lowercase().cmp(&b.name_en.to_lowercase()))
    });

    let total = items.len();
    let items: Vec<Value> = items
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|(i, c)| ingredient_summary(i, c, progress.reviewed_by(&i.id)))
        .collect();

    Ok(json!({"items": items, "total": total, "offset": offset}))
}

fn tool_get_review_progress(state: &AppState, _args: &Value) -> Result<Value, String> {
    let total = state.index.read().ingredients.len();
    let progress = state.progress.lock();
    let human = progress.human_count();
    let ai = progress.ai_count();
    let reviewed = progress.total_reviewed();
    Ok(json!({
        "total": total,
        "reviewed_count": reviewed,
        "human_reviewed_count": human,
        "ai_reviewed_count": ai,
        "remaining": total.saturating_sub(reviewed),
    }))
}

fn tool_mark_reviewed(state: &AppState, args: &Value) -> Result<Value, String> {
    let id = args["ingredient_id"]
        .as_str()
        .ok_or("missing ingredient_id")?;
    let actor = args["actor"].as_str().unwrap_or("claude");
    if !["claude", "human"].contains(&actor) {
        return Err(format!("actor must be 'claude' or 'human', got '{actor}'"));
    }
    if !state.index.read().ingredients.contains_key(id) {
        return Err(format!("ingredient {id} not found"));
    }
    state.progress.lock().mark(id, actor);
    append_history(
        &state.history_path,
        json!({
            "ts": simple_now(), "op": "mark_reviewed", "id": id, "actor": actor,
        }),
    );
    Ok(json!({"ok": true, "ingredient_id": id, "actor": actor}))
}

fn tool_get_dry_run_sample(state: &AppState, args: &Value) -> Result<Value, String> {
    let per_group = args["per_group"].as_u64().unwrap_or(2).clamp(1, 5) as usize;
    let idx = state.index.read();
    let counts = mapping_counts(&idx);
    let progress = state.progress.lock();

    let mut by_group: HashMap<String, Vec<(&Ingredient, u32)>> = HashMap::new();
    for i in idx.ingredients.values() {
        let g = serde_json::to_value(&i.food_group)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| "other".into());
        by_group
            .entry(g)
            .or_default()
            .push((i, counts.get(&i.id).copied().unwrap_or(0)));
    }

    let mut sample: Vec<Value> = Vec::new();
    for (_, mut group) in by_group {
        group.sort_by(|(_, a), (_, b)| b.cmp(a));
        for (i, c) in group.into_iter().take(per_group) {
            sample.push(ingredient_summary(i, c, progress.reviewed_by(&i.id)));
        }
    }
    sample.sort_by(|a, b| {
        b["mapping_count"]
            .as_u64()
            .unwrap_or(0)
            .cmp(&a["mapping_count"].as_u64().unwrap_or(0))
    });

    let count = sample.len();
    Ok(json!({"sample": sample, "count": count}))
}

// ── Ingredient read ───────────────────────────────────────────────────────────

fn tool_get_ingredient(state: &AppState, args: &Value) -> Result<Value, String> {
    let id = args["id"].as_str().ok_or("missing id")?;
    let idx = state.index.read();
    let ingredient = idx
        .ingredients
        .get(id)
        .ok_or_else(|| format!("ingredient {id} not found"))?;
    let mappings: Vec<Value> = idx
        .mappings
        .values()
        .filter(|m| m.canonical_ingredient_id.as_deref() == Some(id))
        .map(|m| serde_json::to_value(m).unwrap_or(json!(null)))
        .collect();
    let relationships: Vec<Value> = idx
        .relationships
        .values()
        .filter(|r| r.raw_id == id)
        .map(|r| serde_json::to_value(r).unwrap_or(json!(null)))
        .collect();
    Ok(json!({
        "ingredient": serde_json::to_value(ingredient).unwrap_or(json!(null)),
        "mappings": mappings,
        "relationships": relationships
    }))
}

fn tool_search_ingredients(state: &AppState, args: &Value) -> Result<Value, String> {
    let q = args["query"].as_str().unwrap_or("").to_lowercase();
    let limit = args["limit"].as_u64().unwrap_or(10) as usize;
    let idx = state.index.read();
    let mut results: Vec<Value> = idx
        .ingredients
        .values()
        .filter(|i| {
            i.name_en.to_lowercase().contains(&q)
                || i.name_fr
                    .as_deref()
                    .map_or(false, |n| n.to_lowercase().contains(&q))
                || i.id.to_lowercase().contains(&q)
        })
        .map(|i| {
            json!({"id": i.id, "name_en": i.name_en, "name_fr": i.name_fr,
                        "food_group": serde_json::to_value(&i.food_group).unwrap_or(json!(null))})
        })
        .collect();
    results.sort_by(|a, b| {
        a["name_en"]
            .as_str()
            .unwrap_or("")
            .cmp(b["name_en"].as_str().unwrap_or(""))
    });
    results.truncate(limit);
    Ok(json!({"results": results}))
}

fn tool_list_ingredients(state: &AppState, args: &Value) -> Result<Value, String> {
    let search = args["search"].as_str().unwrap_or("").to_lowercase();
    let fg_filter = args["food_group"].as_str();
    let sort = args["sort"].as_str().unwrap_or("name");
    let offset = args["offset"].as_u64().unwrap_or(0) as usize;
    let limit = args["limit"].as_u64().unwrap_or(50) as usize;
    let idx = state.index.read();
    let counts = mapping_counts(&idx);
    let progress = state.progress.lock();

    let mut items: Vec<(&Ingredient, u32)> = idx
        .ingredients
        .values()
        .filter(|i| {
            if search.is_empty() {
                return true;
            }
            i.name_en.to_lowercase().contains(&search)
                || i.name_fr
                    .as_deref()
                    .map_or(false, |n| n.to_lowercase().contains(&search))
                || i.id.to_lowercase().contains(&search)
        })
        .filter(|i| {
            fg_filter.map_or(true, |fg| {
                serde_json::to_value(&i.food_group)
                    .ok()
                    .and_then(|v| v.as_str().map(|s| s == fg))
                    .unwrap_or(false)
            })
        })
        .map(|i| (i, counts.get(&i.id).copied().unwrap_or(0)))
        .collect();

    match sort {
        "mapping_count_desc" => {
            items.sort_by(|(a, ca), (b, cb)| cb.cmp(ca).then_with(|| a.name_en.cmp(&b.name_en)))
        }
        _ => {
            items.sort_by(|(a, _), (b, _)| a.name_en.to_lowercase().cmp(&b.name_en.to_lowercase()))
        }
    }

    let total = items.len();
    let items: Vec<Value> = items
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|(i, c)| ingredient_summary(i, c, progress.reviewed_by(&i.id)))
        .collect();

    Ok(json!({"items": items, "total": total}))
}

// ── Ingredient write ──────────────────────────────────────────────────────────

fn tool_save_ingredient(state: &AppState, args: &Value) -> Result<Value, String> {
    let dry_run = args["dry_run"].as_bool().unwrap_or(false);
    let ingredient: Ingredient = serde_json::from_value(args["ingredient"].clone())
        .map_err(|e| format!("invalid ingredient: {e}"))?;
    if dry_run {
        let before = state
            .index
            .read()
            .ingredients
            .get(&ingredient.id)
            .map(|i| serde_json::to_value(i).unwrap_or(json!(null)));
        return Ok(
            json!({"action":"save","dry_run":true,"before":before,"after":serde_json::to_value(&ingredient).unwrap_or(json!(null)),"written":false}),
        );
    }
    save_ingredient_inner(state, ingredient).map_err(|e| e.to_string())
}

fn save_ingredient_inner(state: &AppState, ingredient: Ingredient) -> AppResult<Value> {
    let path = state.repo.ingredient_path(&ingredient.id);
    let as_value = serde_yaml::to_value(&ingredient)?;
    let as_json: serde_json::Value = serde_yaml::from_value(as_value)?;
    state.schemas.validate_ingredient(&path, &as_json)?;
    write_yaml_atomic(&path, &ingredient)?;
    state
        .index
        .write()
        .ingredients
        .insert(ingredient.id.clone(), ingredient.clone());
    Ok(serde_json::to_value(&ingredient).unwrap_or(json!(null)))
}

fn tool_clone_ingredient(state: &AppState, args: &Value) -> Result<Value, String> {
    let dry_run = args["dry_run"].as_bool().unwrap_or(false);
    let source_id = args["source_id"].as_str().ok_or("missing source_id")?;
    let new_id = args["new_id"].as_str().ok_or("missing new_id")?.trim();
    let new_name = args["new_name_en"]
        .as_str()
        .ok_or("missing new_name_en")?
        .trim();
    let new_name_fr = args["new_name_fr"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from);

    if new_id.is_empty() {
        return Err("new_id is empty".into());
    }
    if new_name.is_empty() {
        return Err("new_name_en is empty".into());
    }
    if new_id.len() > 40
        || !new_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err(format!(
            "new_id '{new_id}' must be snake_case, max 40 chars, no hyphens"
        ));
    }

    let fork_count = state
        .session_fork_count
        .load(std::sync::atomic::Ordering::Relaxed);
    if fork_count >= MAX_FORKS_PER_SESSION {
        return Err(format!("session_fork_limit_reached: {fork_count}/{MAX_FORKS_PER_SESSION} forks used. Human review required."));
    }

    let mut clone = {
        let idx = state.index.read();
        if idx.ingredients.contains_key(new_id) {
            return Err(format!("ingredient {new_id} already exists"));
        }
        idx.ingredients
            .get(source_id)
            .cloned()
            .ok_or_else(|| format!("source ingredient {source_id} not found"))?
    };

    let before = serde_json::to_value(&clone).unwrap_or(json!(null));
    clone.id = new_id.to_string();
    clone.name_en = new_name.to_string();
    clone.name_fr = new_name_fr;
    clone.name_en_plural = None;
    clone.name_fr_plural = None;
    clone.notes = None;
    clone.aliases_en = None;
    clone.aliases_fr = None;
    clone.serving_unit_types = None;
    let after = serde_json::to_value(&clone).unwrap_or(json!(null));

    if dry_run {
        return Ok(
            json!({"action":"clone","dry_run":true,"source_id":source_id,"new_id":new_id,"before":before,"after":after,"written":false}),
        );
    }

    let path = state.repo.ingredient_path(&clone.id);
    let as_value = serde_yaml::to_value(&clone).map_err(|e| e.to_string())?;
    let as_json: serde_json::Value = serde_yaml::from_value(as_value).map_err(|e| e.to_string())?;
    state
        .schemas
        .validate_ingredient(&path, &as_json)
        .map_err(|e| e.to_string())?;
    write_yaml_atomic(&path, &clone).map_err(|e| e.to_string())?;
    state
        .index
        .write()
        .ingredients
        .insert(clone.id.clone(), clone.clone());
    state
        .session_fork_count
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    Ok(after)
}

fn tool_delete_ingredient(state: &AppState, args: &Value) -> Result<Value, String> {
    let id = args["id"].as_str().ok_or("missing id")?;
    let dry_run = args["dry_run"].as_bool().unwrap_or(false);
    {
        let idx = state.index.read();
        if !idx.ingredients.contains_key(id) {
            return Err(format!("ingredient {id} not found"));
        }
    }
    if dry_run {
        return Ok(json!({"action":"delete","dry_run":true,"id":id,"written":false}));
    }
    std::fs::remove_file(state.repo.ingredient_path(id))
        .map_err(|e| format!("delete failed: {e}"))?;
    state.index.write().ingredients.remove(id);
    Ok(json!({"ok":true,"id":id}))
}

// ── Generic ingredient field updater ─────────────────────────────────────────

fn update_ingredient<F>(
    state: &AppState,
    id: &str,
    dry_run: bool,
    mutate: F,
) -> Result<Value, String>
where
    F: FnOnce(&mut Ingredient) -> Result<(), String>,
{
    let mut updated = state
        .index
        .read()
        .ingredients
        .get(id)
        .cloned()
        .ok_or_else(|| format!("ingredient {id} not found"))?;

    let before = serde_json::to_value(&updated).unwrap_or(json!(null));
    mutate(&mut updated)?;
    let after = serde_json::to_value(&updated).unwrap_or(json!(null));

    if dry_run {
        return Ok(
            json!({"action":"update","dry_run":true,"id":id,"before":before,"after":after,"written":false}),
        );
    }

    let path = state.repo.ingredient_path(&updated.id);
    let as_value = serde_yaml::to_value(&updated).map_err(|e| e.to_string())?;
    let as_json: serde_json::Value = serde_yaml::from_value(as_value).map_err(|e| e.to_string())?;
    state
        .schemas
        .validate_ingredient(&path, &as_json)
        .map_err(|e| e.to_string())?;
    write_yaml_atomic(&path, &updated).map_err(|e| e.to_string())?;
    state
        .index
        .write()
        .ingredients
        .insert(updated.id.clone(), updated);

    Ok(after)
}

fn tool_set_ingredient_field<F>(
    state: &AppState,
    args: &Value,
    mutate: F,
    _field_key: &str,
) -> Result<Value, String>
where
    F: FnOnce(&mut Ingredient, &Value) -> Result<(), String>,
{
    let id = args["id"].as_str().ok_or("missing id")?;
    let dry_run = args["dry_run"].as_bool().unwrap_or(false);
    let v = args.get("value").unwrap_or(&Value::Null).clone();
    update_ingredient(state, id, dry_run, |i| mutate(i, &v))
}

fn tool_set_ingredient_display_unit(state: &AppState, args: &Value) -> Result<Value, String> {
    let id = args["id"].as_str().ok_or("missing id")?;
    let dry_run = args["dry_run"].as_bool().unwrap_or(false);
    let unit: Option<String> = match args["display_unit"].as_str() {
        None | Some("null") => None,
        Some("g") => Some("g".into()),
        Some("ml") => Some("ml".into()),
        Some(other) => {
            return Err(format!(
                "display_unit must be 'g', 'ml', or null (got {other:?})"
            ))
        }
    };
    if args["display_unit"].is_null() {
        update_ingredient(state, id, dry_run, |i| {
            i.display_unit = None;
            Ok(())
        })
    } else {
        update_ingredient(state, id, dry_run, |i| {
            i.display_unit = unit;
            Ok(())
        })
    }
}

fn tool_set_ingredient_serving_mode(state: &AppState, args: &Value) -> Result<Value, String> {
    let id = args["id"].as_str().ok_or("missing id")?;
    let dry_run = args["dry_run"].as_bool().unwrap_or(false);
    let mode: Option<ServingMode> = if args["serving_mode"].is_null() {
        None
    } else {
        let s = args["serving_mode"]
            .as_str()
            .ok_or("serving_mode must be a string or null")?;
        Some(serde_json::from_value(json!(s)).map_err(|_| format!("unknown serving_mode: {s}"))?)
    };
    update_ingredient(state, id, dry_run, |i| {
        i.serving_mode = mode;
        Ok(())
    })
}

fn tool_set_ingredient_typical_state(state: &AppState, args: &Value) -> Result<Value, String> {
    let id = args["id"].as_str().ok_or("missing id")?;
    let dry_run = args["dry_run"].as_bool().unwrap_or(false);
    let ts: Option<TypicalState> = if args["typical_state"].is_null() {
        None
    } else {
        let s = args["typical_state"]
            .as_str()
            .ok_or("typical_state must be a string or null")?;
        Some(serde_json::from_value(json!(s)).map_err(|_| format!("unknown typical_state: {s}"))?)
    };
    update_ingredient(state, id, dry_run, |i| {
        i.typical_state = ts;
        Ok(())
    })
}

fn tool_set_ingredient_food_group(state: &AppState, args: &Value) -> Result<Value, String> {
    let id = args["id"].as_str().ok_or("missing id")?;
    let dry_run = args["dry_run"].as_bool().unwrap_or(false);
    let fg_str = args["food_group"].as_str().ok_or("missing food_group")?;
    let fg: FoodGroup = serde_json::from_value(json!(fg_str))
        .map_err(|_| format!("unknown food_group: {fg_str}"))?;
    update_ingredient(state, id, dry_run, |i| {
        i.food_group = fg;
        Ok(())
    })
}

fn tool_set_ingredient_state_portion(state: &AppState, args: &Value) -> Result<Value, String> {
    let id = args["id"].as_str().ok_or("missing id")?;
    let dry_run = args["dry_run"].as_bool().unwrap_or(false);
    let state_key = args["state_key"]
        .as_str()
        .ok_or("missing state_key")?
        .to_string();
    let portion_g: Option<f64> = if args["portion_g"].is_null() {
        None
    } else {
        args["portion_g"].as_f64()
    };
    update_ingredient(state, id, dry_run, move |i| {
        i.states.entry(state_key).or_default().portion_g = portion_g;
        Ok(())
    })
}

fn tool_set_ingredient_state_density(state: &AppState, args: &Value) -> Result<Value, String> {
    let id = args["id"].as_str().ok_or("missing id")?;
    let dry_run = args["dry_run"].as_bool().unwrap_or(false);
    let state_key = args["state_key"]
        .as_str()
        .ok_or("missing state_key")?
        .to_string();
    let density_g_ml: Option<f64> = if args["density_g_ml"].is_null() {
        None
    } else {
        args["density_g_ml"].as_f64()
    };
    update_ingredient(state, id, dry_run, move |i| {
        i.states.entry(state_key).or_default().density_g_ml = density_g_ml;
        Ok(())
    })
}

fn tool_set_ingredient_serving_unit_types(state: &AppState, args: &Value) -> Result<Value, String> {
    let id = args["id"].as_str().ok_or("missing id")?;
    let dry_run = args["dry_run"].as_bool().unwrap_or(false);
    // null clears; an empty (or empty-after-dedup) list also clears so we never
    // write `serving_unit_types: []` to YAML.
    let units: Option<Vec<String>> = if args["serving_unit_types"].is_null() {
        None
    } else {
        let raw: Vec<String> = args["serving_unit_types"]
            .as_array()
            .ok_or("serving_unit_types must be an array of strings or null")?
            .iter()
            .map(|v| {
                v.as_str()
                    .map(String::from)
                    .ok_or_else(|| "serving_unit_types entries must be strings".to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        let validated = validate_serving_units(&raw, &state.known_units)?;
        if validated.is_empty() {
            None
        } else {
            Some(validated)
        }
    };
    update_ingredient(state, id, dry_run, move |i| {
        i.serving_unit_types = units;
        Ok(())
    })
}

// ── Generic mapping updater ───────────────────────────────────────────────────

fn parse_source(s: &str) -> Result<Source, String> {
    serde_json::from_value(json!(s)).map_err(|_| format!("unknown source: {s}"))
}

fn update_mapping<F>(
    state: &AppState,
    source: Source,
    source_id: &str,
    dry_run: bool,
    mutate: F,
) -> Result<Value, String>
where
    F: FnOnce(&mut SourceMapping) -> Result<(), String>,
{
    let key = (source, source_id.to_string());
    let mut updated = state
        .index
        .read()
        .mappings
        .get(&key)
        .cloned()
        .ok_or_else(|| format!("mapping {}#{source_id} not found", source.as_dir()))?;

    let before = serde_json::to_value(&updated).unwrap_or(json!(null));
    mutate(&mut updated)?;
    let after = serde_json::to_value(&updated).unwrap_or(json!(null));

    if dry_run {
        return Ok(
            json!({"action":"update_mapping","dry_run":true,"source":source.as_dir(),"source_id":source_id,"before":before,"after":after,"written":false}),
        );
    }

    let path = state.repo.mapping_path(source.as_dir(), source_id);
    let as_value = serde_yaml::to_value(&updated).map_err(|e| e.to_string())?;
    let as_json: serde_json::Value = serde_yaml::from_value(as_value).map_err(|e| e.to_string())?;
    state
        .schemas
        .validate_source_mapping(&path, &as_json)
        .map_err(|e| e.to_string())?;
    write_yaml_atomic(&path, &updated).map_err(|e| e.to_string())?;
    state.index.write().mappings.insert(key, updated.clone());

    Ok(after)
}

fn tool_set_mapping_cooking_method(state: &AppState, args: &Value) -> Result<Value, String> {
    let source = parse_source(args["source"].as_str().ok_or("missing source")?)?;
    let source_id = args["source_id"].as_str().ok_or("missing source_id")?;
    let dry_run = args["dry_run"].as_bool().unwrap_or(false);
    let cm_str = args["cooking_method"]
        .as_str()
        .ok_or("missing cooking_method")?;
    let cm: SourceMappingCookingMethod = serde_json::from_value(json!(cm_str))
        .map_err(|_| format!("unknown cooking_method: {cm_str}"))?;
    update_mapping(state, source, source_id, dry_run, |m| {
        m.cooking_method = Some(cm);
        Ok(())
    })
}

fn tool_link_mapping(state: &AppState, args: &Value) -> Result<Value, String> {
    let source = parse_source(args["source"].as_str().ok_or("missing source")?)?;
    let source_id = args["source_id"].as_str().ok_or("missing source_id")?;
    let ingredient_id = args["ingredient_id"]
        .as_str()
        .ok_or("missing ingredient_id")?;
    let dry_run = args["dry_run"].as_bool().unwrap_or(false);
    let actor = args["actor"].as_str().unwrap_or("claude");

    if !state.index.read().ingredients.contains_key(ingredient_id) {
        return Err(format!("target ingredient {ingredient_id} not found"));
    }

    let (match_method, confidence): (&str, f64) = match actor {
        "human" => ("manual", 1.0),
        _ => ("ai_review", 0.9),
    };

    let result = update_mapping(state, source, source_id, dry_run, |m| {
        m.canonical_ingredient_id = Some(ingredient_id.to_string());
        m.match_method = Some(match_method.into());
        m.confidence = Some(confidence);
        Ok(())
    })?;

    if !dry_run {
        append_history(
            &state.history_path,
            json!({
                "ts": simple_now(), "op": "link_mapping",
                "source": source.as_dir(), "source_id": source_id,
                "ingredient_id": ingredient_id,
                "actor": actor, "confidence": confidence, "match_method": match_method,
            }),
        );
    }
    Ok(result)
}

fn tool_unlink_mapping(state: &AppState, args: &Value) -> Result<Value, String> {
    let source = parse_source(args["source"].as_str().ok_or("missing source")?)?;
    let source_id = args["source_id"].as_str().ok_or("missing source_id")?;
    let dry_run = args["dry_run"].as_bool().unwrap_or(false);
    update_mapping(state, source, source_id, dry_run, |m| {
        m.canonical_ingredient_id = None;
        Ok(())
    })
}

fn tool_mark_mapping_composite(state: &AppState, args: &Value) -> Result<Value, String> {
    let source = parse_source(args["source"].as_str().ok_or("missing source")?)?;
    let source_id = args["source_id"].as_str().ok_or("missing source_id")?;
    let dry_run = args["dry_run"].as_bool().unwrap_or(false);
    update_mapping(state, source, source_id, dry_run, |m| {
        m.canonical_ingredient_id = Some(SourceMapping::COMPOSITE_SENTINEL.into());
        Ok(())
    })
}

fn tool_validate_mapping(state: &AppState, args: &Value) -> Result<Value, String> {
    let source = parse_source(args["source"].as_str().ok_or("missing source")?)?;
    let source_id = args["source_id"].as_str().ok_or("missing source_id")?;
    let dry_run = args["dry_run"].as_bool().unwrap_or(false);
    update_mapping(state, source, source_id, dry_run, |m| {
        if !m.is_linked() {
            return Err("cannot validate an unlinked or composite mapping".into());
        }
        m.match_method = Some("manual".into());
        m.confidence = Some(1.0);
        Ok(())
    })
}

fn tool_search_unlinked_mappings(state: &AppState, args: &Value) -> Result<Value, String> {
    let source_filter: Option<Source> = args["source"].as_str().and_then(|s| parse_source(s).ok());
    let q = args["query"].as_str().unwrap_or("").to_lowercase();
    let limit = args["limit"].as_u64().unwrap_or(20) as usize;
    let idx = state.index.read();
    let mut results: Vec<Value> = idx
        .mappings
        .values()
        .filter(|m| m.is_unlinked())
        .filter(|m| source_filter.map_or(true, |s| m.source == s))
        .filter(|m| {
            if q.is_empty() {
                return true;
            }
            m.label
                .as_deref()
                .map_or(false, |l| l.to_lowercase().contains(&q))
                || m.source_id.to_lowercase().contains(&q)
        })
        .map(|m| serde_json::to_value(m).unwrap_or(json!(null)))
        .collect();
    results.sort_by(|a, b| {
        a["label"]
            .as_str()
            .unwrap_or("")
            .cmp(b["label"].as_str().unwrap_or(""))
    });
    results.truncate(limit);
    Ok(json!({"results": results}))
}

// ── Translation / aliases tools ───────────────────────────────────────────────

fn tool_set_ingredient_name_fr(state: &AppState, args: &Value) -> Result<Value, String> {
    let id = args["id"].as_str().ok_or("missing id")?;
    let dry_run = args["dry_run"].as_bool().unwrap_or(false);
    let name_fr = if args["name_fr"].is_null() {
        None
    } else {
        args["name_fr"].as_str().map(String::from)
    };
    update_ingredient(state, id, dry_run, |i| {
        i.name_fr = name_fr;
        Ok(())
    })
}

fn tool_set_ingredient_name_en(state: &AppState, args: &Value) -> Result<Value, String> {
    let id = args["id"].as_str().ok_or("missing id")?;
    let dry_run = args["dry_run"].as_bool().unwrap_or(false);
    let name_en = args["name_en"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or("name_en is required and cannot be empty")?
        .to_string();
    update_ingredient(state, id, dry_run, |i| {
        i.name_en = name_en;
        Ok(())
    })
}

fn tool_set_ingredient_aliases(state: &AppState, args: &Value) -> Result<Value, String> {
    let id = args["id"].as_str().ok_or("missing id")?;
    let dry_run = args["dry_run"].as_bool().unwrap_or(false);
    let lang = args["lang"].as_str().ok_or("missing lang (en|fr)")?;
    let aliases: Option<Vec<String>> = if args["aliases"].is_null() {
        None
    } else {
        args["aliases"].as_array().map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
    };
    match lang {
        "en" => update_ingredient(state, id, dry_run, |i| {
            i.aliases_en = aliases;
            Ok(())
        }),
        "fr" => update_ingredient(state, id, dry_run, |i| {
            i.aliases_fr = aliases;
            Ok(())
        }),
        _ => Err(format!("lang must be 'en' or 'fr', got '{lang}'")),
    }
}

fn tool_set_ingredient_plural(state: &AppState, args: &Value) -> Result<Value, String> {
    let id = args["id"].as_str().ok_or("missing id")?;
    let dry_run = args["dry_run"].as_bool().unwrap_or(false);
    let lang = args["lang"].as_str().ok_or("missing lang (en|fr)")?;
    let plural = if args["plural"].is_null() {
        None
    } else {
        args["plural"].as_str().map(String::from)
    };
    match lang {
        "en" => update_ingredient(state, id, dry_run, |i| {
            i.name_en_plural = plural;
            Ok(())
        }),
        "fr" => update_ingredient(state, id, dry_run, |i| {
            i.name_fr_plural = plural;
            Ok(())
        }),
        _ => Err(format!("lang must be 'en' or 'fr', got '{lang}'")),
    }
}

fn tool_set_ingredient_unit_label(state: &AppState, args: &Value) -> Result<Value, String> {
    let id = args["id"].as_str().ok_or("missing id")?;
    let dry_run = args["dry_run"].as_bool().unwrap_or(false);
    let lang = args["lang"].as_str().ok_or("missing lang (en|fr)")?;
    let label = if args["unit_label"].is_null() {
        None
    } else {
        args["unit_label"].as_str().map(String::from)
    };
    match lang {
        "en" => update_ingredient(state, id, dry_run, |i| {
            i.unit_label_en = label;
            Ok(())
        }),
        "fr" => update_ingredient(state, id, dry_run, |i| {
            i.unit_label_fr = label;
            Ok(())
        }),
        _ => Err(format!("lang must be 'en' or 'fr', got '{lang}'")),
    }
}

// ── Bulk review application ───────────────────────────────────────────────────

fn opt_string(v: &Value) -> Result<Option<String>, String> {
    match v {
        Value::Null => Ok(None),
        Value::String(s) => Ok(Some(s.clone())),
        _ => Err("expected a string or null".into()),
    }
}

fn opt_bool_strict(v: &Value) -> Result<Option<bool>, String> {
    match v {
        Value::Null => Ok(None),
        Value::Bool(b) => Ok(Some(*b)),
        _ => Err("expected a boolean or null".into()),
    }
}

/// null → None; [] → None (never write empty lists to YAML); array → Some.
fn opt_string_vec(v: &Value) -> Result<Option<Vec<String>>, String> {
    match v {
        Value::Null => Ok(None),
        Value::Array(arr) => {
            let items = arr
                .iter()
                .map(|x| {
                    x.as_str()
                        .map(String::from)
                        .ok_or_else(|| "array entries must be strings".to_string())
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(if items.is_empty() { None } else { Some(items) })
        }
        _ => Err("expected an array of strings or null".into()),
    }
}

fn opt_enum<T: serde::de::DeserializeOwned>(v: &Value, what: &str) -> Result<Option<T>, String> {
    if v.is_null() {
        return Ok(None);
    }
    serde_json::from_value(v.clone())
        .map(Some)
        .map_err(|_| format!("unknown {what}: {v}"))
}

/// Apply one `fields` patch entry to an in-memory ingredient. Mirrors the
/// individual set_ingredient_* tools — same validation, same null-clears
/// semantics — so apply_review stays behaviourally identical to a setter chain.
fn apply_ingredient_field(
    i: &mut Ingredient,
    key: &str,
    v: &Value,
    known_units: &HashSet<String>,
) -> Result<(), String> {
    match key {
        "name_en" => {
            i.name_en = v
                .as_str()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or("name_en cannot be null or empty")?
                .to_string();
        }
        "name_fr" => i.name_fr = opt_string(v)?,
        "name_en_plural" => i.name_en_plural = opt_string(v)?,
        "name_fr_plural" => i.name_fr_plural = opt_string(v)?,
        "unit_label_en" => i.unit_label_en = opt_string(v)?,
        "unit_label_fr" => i.unit_label_fr = opt_string(v)?,
        "can_eat_raw" => i.can_eat_raw = opt_bool_strict(v)?,
        "can_eat_cooked" => i.can_eat_cooked = opt_bool_strict(v)?,
        "display_unit" => {
            i.display_unit = match v {
                Value::Null => None,
                Value::String(s) if s == "g" || s == "ml" => Some(s.clone()),
                other => {
                    return Err(format!("display_unit must be 'g', 'ml', or null (got {other})"))
                }
            };
        }
        "serving_mode" => i.serving_mode = opt_enum(v, "serving_mode")?,
        "typical_state" => i.typical_state = opt_enum(v, "typical_state")?,
        "food_group" => {
            i.food_group = serde_json::from_value(v.clone())
                .map_err(|_| format!("unknown food_group: {v}"))?;
        }
        "aliases_en" => i.aliases_en = opt_string_vec(v)?,
        "aliases_fr" => i.aliases_fr = opt_string_vec(v)?,
        "serving_unit_types" => {
            i.serving_unit_types = match opt_string_vec(v)? {
                None => None,
                Some(raw) => {
                    let validated = validate_serving_units(&raw, known_units)?;
                    if validated.is_empty() {
                        None
                    } else {
                        Some(validated)
                    }
                }
            };
        }
        other => {
            return Err(format!(
                "unknown field '{other}'. Allowed: name_en, name_fr, name_en_plural, \
                 name_fr_plural, unit_label_en, unit_label_fr, can_eat_raw, can_eat_cooked, \
                 display_unit, serving_mode, typical_state, food_group, aliases_en, aliases_fr, \
                 serving_unit_types"
            ))
        }
    }
    Ok(())
}

enum MappingOpKind {
    Link { target: String },
    Unlink,
    Composite,
    SetCookingMethod(SourceMappingCookingMethod),
}

struct MappingOp {
    source: Source,
    source_id: String,
    kind: MappingOpKind,
}

fn op_action_name(kind: &MappingOpKind) -> &'static str {
    match kind {
        MappingOpKind::Link { .. } => "link",
        MappingOpKind::Unlink => "unlink",
        MappingOpKind::Composite => "composite",
        MappingOpKind::SetCookingMethod(_) => "set_cooking_method",
    }
}

fn op_preview(op: &MappingOp) -> Value {
    let mut v = json!({
        "op": op_action_name(&op.kind),
        "source": op.source.as_dir(),
        "source_id": op.source_id,
    });
    match &op.kind {
        MappingOpKind::Link { target } => v["ingredient_id"] = json!(target),
        MappingOpKind::SetCookingMethod(cm) => {
            v["cooking_method"] = serde_json::to_value(cm).unwrap_or(json!(null))
        }
        _ => {}
    }
    v
}

/// Parse and pre-validate mapping_ops: every referenced mapping must exist,
/// link targets must exist, enums must parse — all before any write happens.
fn parse_mapping_ops(state: &AppState, v: &Value) -> Result<Vec<MappingOp>, String> {
    let arr = match v {
        Value::Null => return Ok(Vec::new()),
        Value::Array(a) => a,
        _ => return Err("mapping_ops must be an array".into()),
    };
    let idx = state.index.read();
    let mut ops = Vec::with_capacity(arr.len());
    for (n, item) in arr.iter().enumerate() {
        let op_name = item["op"]
            .as_str()
            .ok_or(format!("mapping_ops[{n}]: missing op"))?;
        let source = parse_source(
            item["source"]
                .as_str()
                .ok_or(format!("mapping_ops[{n}]: missing source"))?,
        )?;
        let source_id = item["source_id"]
            .as_str()
            .ok_or(format!("mapping_ops[{n}]: missing source_id"))?
            .to_string();
        if !idx.mappings.contains_key(&(source, source_id.clone())) {
            return Err(format!(
                "mapping_ops[{n}]: mapping {}#{source_id} not found",
                source.as_dir()
            ));
        }
        let kind = match op_name {
            "link" => {
                let target = item["ingredient_id"]
                    .as_str()
                    .ok_or(format!("mapping_ops[{n}]: link requires ingredient_id"))?
                    .to_string();
                if !idx.ingredients.contains_key(&target) {
                    return Err(format!(
                        "mapping_ops[{n}]: target ingredient {target} not found"
                    ));
                }
                MappingOpKind::Link { target }
            }
            "unlink" => MappingOpKind::Unlink,
            "composite" => MappingOpKind::Composite,
            "set_cooking_method" => {
                let cm_str = item["cooking_method"].as_str().ok_or(format!(
                    "mapping_ops[{n}]: set_cooking_method requires cooking_method"
                ))?;
                let cm = serde_json::from_value(json!(cm_str))
                    .map_err(|_| format!("mapping_ops[{n}]: unknown cooking_method: {cm_str}"))?;
                MappingOpKind::SetCookingMethod(cm)
            }
            other => {
                return Err(format!(
                    "mapping_ops[{n}]: unknown op '{other}' (link|unlink|composite|set_cooking_method)"
                ))
            }
        };
        ops.push(MappingOp {
            source,
            source_id,
            kind,
        });
    }
    Ok(ops)
}

fn tool_apply_review(state: &AppState, args: &Value) -> Result<Value, String> {
    let id = args["ingredient_id"]
        .as_str()
        .ok_or("missing ingredient_id")?;
    let dry_run = args["dry_run"].as_bool().unwrap_or(false);
    let actor = args["actor"].as_str().unwrap_or("claude");
    if !["claude", "human"].contains(&actor) {
        return Err(format!("actor must be 'claude' or 'human', got '{actor}'"));
    }
    let mark = args["mark_reviewed"].as_bool().unwrap_or(false);

    let mut updated = state
        .index
        .read()
        .ingredients
        .get(id)
        .cloned()
        .ok_or_else(|| format!("ingredient {id} not found"))?;
    let before = serde_json::to_value(&updated).unwrap_or(json!(null));

    // 1) Field patches (in-memory; validated per field, schema-validated once at write).
    let mut field_log: Vec<(String, Value)> = Vec::new();
    if let Some(fields) = args.get("fields").filter(|v| !v.is_null()) {
        let obj = fields.as_object().ok_or("fields must be an object")?;
        for (k, v) in obj {
            apply_ingredient_field(&mut updated, k, v, &state.known_units)
                .map_err(|e| format!("fields.{k}: {e}"))?;
            field_log.push((k.clone(), v.clone()));
        }
    }

    // 2) State patches.
    if let Some(states) = args.get("states").filter(|v| !v.is_null()) {
        let obj = states.as_object().ok_or("states must be an object")?;
        for (sk, patch) in obj {
            if !["raw", "cooked", "processed"].contains(&sk.as_str()) {
                return Err(format!("states.{sk}: state_key must be raw|cooked|processed"));
            }
            let p = patch
                .as_object()
                .ok_or_else(|| format!("states.{sk}: must be an object"))?;
            let entry = updated.states.entry(sk.clone()).or_default();
            for (pk, pv) in p {
                let parsed = if pv.is_null() {
                    None
                } else {
                    Some(pv.as_f64().ok_or_else(|| {
                        format!("states.{sk}.{pk} must be a number or null")
                    })?)
                };
                match pk.as_str() {
                    "portion_g" => entry.portion_g = parsed,
                    "density_g_ml" => entry.density_g_ml = parsed,
                    other => {
                        return Err(format!(
                            "states.{sk}: unknown key '{other}' (portion_g|density_g_ml)"
                        ))
                    }
                }
            }
            field_log.push((format!("states.{sk}"), patch.clone()));
        }
    }

    // 3) Mapping ops — parsed and pre-validated before any write.
    let ops = parse_mapping_ops(state, args.get("mapping_ops").unwrap_or(&Value::Null))?;

    let after = serde_json::to_value(&updated).unwrap_or(json!(null));
    let ingredient_changed = !field_log.is_empty();

    if dry_run {
        let ops_preview: Vec<Value> = ops.iter().map(op_preview).collect();
        return Ok(json!({
            "action": "apply_review", "dry_run": true, "id": id,
            "before": before, "after": after,
            "mapping_ops": ops_preview,
            "would_mark_reviewed": mark, "written": false
        }));
    }

    // Single schema-validation + single YAML write for the ingredient.
    if ingredient_changed {
        let path = state.repo.ingredient_path(&updated.id);
        let as_value = serde_yaml::to_value(&updated).map_err(|e| e.to_string())?;
        let as_json: serde_json::Value =
            serde_yaml::from_value(as_value).map_err(|e| e.to_string())?;
        state
            .schemas
            .validate_ingredient(&path, &as_json)
            .map_err(|e| e.to_string())?;
        write_yaml_atomic(&path, &updated).map_err(|e| e.to_string())?;
        state
            .index
            .write()
            .ingredients
            .insert(updated.id.clone(), updated.clone());
        for (field, value) in &field_log {
            append_history(
                &state.history_path,
                json!({
                    "ts": simple_now(), "op": "apply_review.set", "id": id,
                    "field": field, "value": value, "actor": actor,
                }),
            );
        }
    }

    let (match_method, confidence): (&str, f64) = match actor {
        "human" => ("manual", 1.0),
        _ => ("ai_review", 0.9),
    };
    let mut applied: Vec<Value> = Vec::new();
    for op in &ops {
        let result = match &op.kind {
            MappingOpKind::Link { target } => {
                let t = target.clone();
                update_mapping(state, op.source, &op.source_id, false, |m| {
                    m.canonical_ingredient_id = Some(t);
                    m.match_method = Some(match_method.into());
                    m.confidence = Some(confidence);
                    Ok(())
                })
            }
            MappingOpKind::Unlink => update_mapping(state, op.source, &op.source_id, false, |m| {
                m.canonical_ingredient_id = None;
                Ok(())
            }),
            MappingOpKind::Composite => {
                update_mapping(state, op.source, &op.source_id, false, |m| {
                    m.canonical_ingredient_id = Some(SourceMapping::COMPOSITE_SENTINEL.into());
                    Ok(())
                })
            }
            MappingOpKind::SetCookingMethod(cm) => {
                let cm = *cm;
                update_mapping(state, op.source, &op.source_id, false, move |m| {
                    m.cooking_method = Some(cm);
                    Ok(())
                })
            }
        };
        match result {
            Ok(_) => {
                let mut entry = json!({
                    "ts": simple_now(), "op": "apply_review.mapping",
                    "source": op.source.as_dir(), "source_id": op.source_id,
                    "action": op_action_name(&op.kind), "actor": actor,
                });
                if let MappingOpKind::Link { target } = &op.kind {
                    entry["ingredient_id"] = json!(target);
                    entry["confidence"] = json!(confidence);
                    entry["match_method"] = json!(match_method);
                }
                append_history(&state.history_path, entry);
                applied.push(op_preview(op));
            }
            Err(e) => {
                return Err(format!(
                    "mapping op {}#{} ({}) failed after {} prior op(s) applied (ingredient write {}): {e}",
                    op.source.as_dir(),
                    op.source_id,
                    op_action_name(&op.kind),
                    applied.len(),
                    if ingredient_changed { "done" } else { "skipped" }
                ));
            }
        }
    }

    // Mark reviewed last, only after everything succeeded.
    if mark {
        state.progress.lock().mark(id, actor);
        append_history(
            &state.history_path,
            json!({"ts": simple_now(), "op": "mark_reviewed", "id": id, "actor": actor}),
        );
    }

    Ok(json!({
        "ok": true, "id": id,
        "fields_applied": field_log.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
        "mappings_applied": applied,
        "marked_reviewed": mark,
        "ingredient": if ingredient_changed { after } else { json!(null) },
    }))
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn known() -> HashSet<String> {
        FALLBACK_SERVING_UNITS
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    #[test]
    fn accepts_known_units_and_dedups_preserving_order() {
        let units = vec!["glass".to_string(), "cup".to_string(), "glass".to_string()];
        let out = validate_serving_units(&units, &known()).unwrap();
        assert_eq!(out, vec!["glass".to_string(), "cup".to_string()]);
    }

    #[test]
    fn rejects_unknown_unit_and_lists_valid_ones() {
        let units = vec!["teaspoon".to_string(), "mug".to_string()];
        let err = validate_serving_units(&units, &known()).unwrap_err();
        assert!(err.contains("unknown serving unit 'mug'"), "got: {err}");
        assert!(
            err.contains("teaspoon"),
            "error should list valid units: {err}"
        );
    }

    #[test]
    fn empty_list_validates_to_empty() {
        let out = validate_serving_units(&[], &known()).unwrap();
        assert!(out.is_empty());
    }

    fn sample_ingredient() -> Ingredient {
        Ingredient {
            id: "test".into(),
            name_en: "test".into(),
            name_fr: None,
            name_en_plural: None,
            name_fr_plural: None,
            serving_mode: None,
            unit_label_en: None,
            unit_label_fr: None,
            food_group: FoodGroup::Other,
            states: std::collections::BTreeMap::new(),
            notes: None,
            can_eat_raw: None,
            can_eat_cooked: None,
            typical_state: None,
            aliases_en: None,
            aliases_fr: None,
            serving_unit_types: None,
            display_unit: None,
            absorption_factors: None,
        }
    }

    #[test]
    fn apply_field_sets_and_clears_values() {
        let mut i = sample_ingredient();
        apply_ingredient_field(&mut i, "food_group", &json!("meat"), &known()).unwrap();
        assert_eq!(i.food_group, FoodGroup::Meat);
        apply_ingredient_field(&mut i, "can_eat_raw", &json!(false), &known()).unwrap();
        assert_eq!(i.can_eat_raw, Some(false));
        apply_ingredient_field(&mut i, "name_fr", &json!("essai"), &known()).unwrap();
        assert_eq!(i.name_fr.as_deref(), Some("essai"));
        apply_ingredient_field(&mut i, "name_fr", &Value::Null, &known()).unwrap();
        assert_eq!(i.name_fr, None);
        apply_ingredient_field(&mut i, "serving_mode", &json!("item"), &known()).unwrap();
        assert_eq!(i.serving_mode, Some(ServingMode::Item));
    }

    #[test]
    fn apply_field_rejects_bad_values() {
        let mut i = sample_ingredient();
        assert!(apply_ingredient_field(&mut i, "display_unit", &json!("unit"), &known()).is_err());
        assert!(apply_ingredient_field(&mut i, "food_group", &json!("not_a_group"), &known()).is_err());
        assert!(apply_ingredient_field(&mut i, "name_en", &Value::Null, &known()).is_err());
        assert!(apply_ingredient_field(&mut i, "no_such_field", &json!(1), &known()).is_err());
        assert!(
            apply_ingredient_field(&mut i, "serving_unit_types", &json!(["mug"]), &known())
                .is_err()
        );
    }

    #[test]
    fn apply_field_validates_serving_units_and_clears_empty() {
        let mut i = sample_ingredient();
        apply_ingredient_field(
            &mut i,
            "serving_unit_types",
            &json!(["teaspoon", "cup"]),
            &known(),
        )
        .unwrap();
        assert_eq!(
            i.serving_unit_types,
            Some(vec!["teaspoon".to_string(), "cup".to_string()])
        );
        apply_ingredient_field(&mut i, "serving_unit_types", &json!([]), &known()).unwrap();
        assert_eq!(i.serving_unit_types, None);
    }
}
