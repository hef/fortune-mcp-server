use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use std::fs;
use std::path::PathBuf;
use std::collections::HashMap;
use anyhow::{Result, Context};
use chrono::Local;
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;

#[derive(Debug, Serialize, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    params: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize, Deserialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

#[derive(Debug, Clone)]
struct Fortune {
    text: String,
    database: String,
}

struct McpServer {
    name: String,
    version: String,
    fortunes: Vec<Fortune>,
    databases: HashMap<String, Vec<usize>>, // Maps database name to fortune indices
}

impl McpServer {
    fn new(name: String, version: String) -> Self {
        let fortunes = Self::load_fortunes().unwrap_or_else(|e| {
            eprintln!("Warning: Failed to load fortune files: {}. Using default fortunes.", e);
            vec![
                "A beautiful, smart, and loving person will be coming into your life.",
                "A golden egg of opportunity falls into your lap this month.",
                "A smile is your passport into the hearts of others.",
                "Good news will come to you by mail.",
                "The fortune you seek is in another cookie.",
                "You will be hungry again in one hour.",
                "An exciting opportunity lies ahead of you.",
                "You will make many changes before settling down happily.",
                "A thrilling time is in your immediate future.",
                "Your luck has been completely changed today.",
                "You will discover your hidden talents.",
                "The best is yet to come.",
                "Your hard work will soon pay off.",
                "Adventure can be real happiness.",
                "Patience is your ally at the moment. Don't worry!",
            ].iter().map(|text| Fortune {
                text: text.to_string(),
                database: "default".to_string(),
            }).collect()
        });

        // Build database index
        let mut databases: HashMap<String, Vec<usize>> = HashMap::new();
        for (idx, fortune) in fortunes.iter().enumerate() {
            databases.entry(fortune.database.clone())
                .or_insert_with(Vec::new)
                .push(idx);
        }

        Self { name, version, fortunes, databases }
    }

    fn load_fortunes() -> Result<Vec<Fortune>> {
        let mut all_fortunes = Vec::new();

        // Check common fortune file locations
        let fortune_dirs = vec![
            PathBuf::from("./fortunes"),
            PathBuf::from("/usr/share/games/fortunes"),
            PathBuf::from("/usr/share/fortune"),
        ];

        for dir in fortune_dirs {
            if dir.exists() && dir.is_dir() {
                eprintln!("Loading fortunes from: {}", dir.display());
                let fortunes = Self::load_fortunes_from_dir(&dir)?;
                all_fortunes.extend(fortunes);
                if !all_fortunes.is_empty() {
                    break; // Use first directory that has fortunes
                }
            }
        }

        if all_fortunes.is_empty() {
            anyhow::bail!("No fortune files found");
        }

        Ok(all_fortunes)
    }

    fn load_fortunes_from_dir(dir: &PathBuf) -> Result<Vec<Fortune>> {
        let mut fortunes = Vec::new();

        for entry in fs::read_dir(dir).context("Failed to read fortune directory")? {
            let entry = entry?;
            let path = entry.path();

            // Skip .dat files and hidden files
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.ends_with(".dat") || name.starts_with('.') {
                    continue;
                }
            }

            // Only process regular files
            if path.is_file() {
                let database_name = path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                match Self::parse_fortune_file(&path, &database_name) {
                    Ok(mut file_fortunes) => {
                        eprintln!("Loaded {} fortunes from {}", file_fortunes.len(), path.display());
                        fortunes.append(&mut file_fortunes);
                    }
                    Err(e) => {
                        eprintln!("Warning: Failed to parse {}: {}", path.display(), e);
                    }
                }
            }
        }

        Ok(fortunes)
    }

    fn parse_fortune_file(path: &PathBuf, database: &str) -> Result<Vec<Fortune>> {
        let content = fs::read_to_string(path)
            .context("Failed to read fortune file")?;

        let fortunes: Vec<Fortune> = content
            .split("\n%\n")
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|text| Fortune {
                text: text.to_string(),
                database: database.to_string(),
            })
            .collect();

        Ok(fortunes)
    }

    fn is_offensive_database(db_name: &str) -> bool {
        // Common offensive fortune database names
        matches!(db_name, "offensive" | "limerick" | "sex" | "racist" | "ethnic")
    }

    fn filter_fortunes(&self, indices: &[usize], short_only: bool, allow_offensive: bool) -> Vec<usize> {
        indices.iter()
            .copied()
            .filter(|&idx| {
                let fortune = &self.fortunes[idx];

                // Filter by offensive content
                if !allow_offensive && Self::is_offensive_database(&fortune.database) {
                    return false;
                }

                // Filter by length
                if short_only && fortune.text.len() > 160 {
                    return false;
                }

                true
            })
            .collect()
    }

    fn get_fortune_from_database(&self, database: &str, short_only: bool, allow_offensive: bool) -> Result<&Fortune, JsonRpcError> {
        let indices = self.databases.get(database).ok_or_else(|| JsonRpcError {
            code: -32602,
            message: format!("Unknown database: {}. Available databases: {}",
                database,
                self.databases.keys().map(|k| k.as_str()).collect::<Vec<_>>().join(", ")),
            data: None,
        })?;

        if indices.is_empty() {
            return Err(JsonRpcError {
                code: -32602,
                message: format!("Database {} is empty", database),
                data: None,
            });
        }

        let filtered = self.filter_fortunes(indices, short_only, allow_offensive);

        if filtered.is_empty() {
            return Err(JsonRpcError {
                code: -32602,
                message: format!("No fortunes match the specified filters in database {}", database),
                data: None,
            });
        }

        let filtered = self.filter_fortunes(indices, short_only, allow_offensive);

        if filtered.is_empty() {
            return Err(JsonRpcError {
                code: -32602,
                message: format!("No fortunes match the specified filters in database {}", database),
                data: None,
            });
        }

        // Get today's date at midnight as seed
        let now = Local::now();
        let date = now.date_naive();
        let seed = date.and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp() as u64;

        // Use deterministic RNG with date seed and database name
        let db_seed = seed.wrapping_add(database.bytes().map(|b| b as u64).sum::<u64>());
        let mut rng = StdRng::seed_from_u64(db_seed);
        let idx = rng.gen_range(0..filtered.len());

        Ok(&self.fortunes[filtered[idx]])
    }

    fn get_filtered_fortune(&self, database: Option<&str>, short_only: bool, allow_offensive: bool) -> &Fortune {
        let all_indices: Vec<usize> = if let Some(db) = database {
            self.databases.get(db).map(|v| v.clone()).unwrap_or_default()
        } else {
            (0..self.fortunes.len()).collect()
        };

        let filtered = self.filter_fortunes(&all_indices, short_only, allow_offensive);

        let indices = if filtered.is_empty() {
            &all_indices
        } else {
            &filtered
        };

        // Get today's date at midnight as seed
        let now = Local::now();
        let date = now.date_naive();
        let seed = date.and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp() as u64;

        // Use deterministic RNG with date seed
        let mut rng = StdRng::seed_from_u64(seed);
        let index = rng.gen_range(0..indices.len());

        &self.fortunes[indices[index]]
    }

    fn handle_request(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        let result = match request.method.as_str() {
            "initialize" => self.handle_initialize(request.params),
            "tools/list" => self.handle_tools_list(),
            "tools/call" => self.handle_tools_call(request.params),
            "prompts/list" => self.handle_prompts_list(),
            "resources/list" => self.handle_resources_list(),
            _ => Err(JsonRpcError {
                code: -32601,
                message: format!("Method not found: {}", request.method),
                data: None,
            }),
        };

        match result {
            Ok(result) => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: Some(result),
                error: None,
            },
            Err(error) => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: None,
                error: Some(error),
            },
        }
    }

    fn handle_initialize(&self, _params: Option<Value>) -> Result<Value, JsonRpcError> {
        Ok(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {},
                "prompts": {},
                "resources": {}
            },
            "serverInfo": {
                "name": self.name,
                "version": self.version
            }
        }))
    }

    fn handle_tools_list(&self) -> Result<Value, JsonRpcError> {
        let available_dbs: Vec<String> = self.databases.keys().cloned().collect();

        Ok(json!({
            "tools": [
                {
                    "name": "get_fortune",
                    "description": format!("Get today's fortune. The same fortune is shown to all users for the entire day, resetting at midnight. Optionally specify a database. Available databases: {}", available_dbs.join(", ")),
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "database": {
                                "type": "string",
                                "description": format!("Optional: specific fortune database to use. Available: {}", available_dbs.join(", ")),
                                "enum": available_dbs
                            },
                            "short": {
                                "type": "boolean",
                                "description": "Optional: only show short fortunes (160 characters or less)"
                            },
                            "offensive": {
                                "type": "boolean",
                                "description": "Optional: include potentially offensive fortunes"
                            }
                        }
                    }
                }
            ]
        }))
    }

    fn handle_tools_call(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let params = params.ok_or_else(|| JsonRpcError {
            code: -32602,
            message: "Invalid params".to_string(),
            data: None,
        })?;

        let tool_name = params["name"].as_str().ok_or_else(|| JsonRpcError {
            code: -32602,
            message: "Missing tool name".to_string(),
            data: None,
        })?;

        match tool_name {
            "get_fortune" => {
                let args = params.get("arguments");
                let database = args.and_then(|a| a.get("database")).and_then(|v| v.as_str());
                let short_only = args.and_then(|a| a.get("short")).and_then(|v| v.as_bool()).unwrap_or(false);
                let allow_offensive = args.and_then(|a| a.get("offensive")).and_then(|v| v.as_bool()).unwrap_or(false);

                let fortune = if let Some(db) = database {
                    self.get_fortune_from_database(db, short_only, allow_offensive)?
                } else {
                    self.get_filtered_fortune(None, short_only, allow_offensive)
                };

                Ok(json!({
                    "content": [
                        {
                            "type": "text",
                            "text": format!("{}\n\n(from: {})", fortune.text, fortune.database)
                        }
                    ]
                }))
            }
            _ => Err(JsonRpcError {
                code: -32602,
                message: format!("Unknown tool: {}", tool_name),
                data: None,
            }),
        }
    }

    fn handle_prompts_list(&self) -> Result<Value, JsonRpcError> {
        Ok(json!({
            "prompts": []
        }))
    }

    fn handle_resources_list(&self) -> Result<Value, JsonRpcError> {
        Ok(json!({
            "resources": []
        }))
    }
}

fn main() -> Result<()> {
    let server = McpServer::new(
        "fortune-mcp-server".to_string(),
        "0.1.0".to_string(),
    );

    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let reader = stdin.lock();

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let request: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(req) => req,
            Err(e) => {
                let error_response = JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: None,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32700,
                        message: format!("Parse error: {}", e),
                        data: None,
                    }),
                };
                let response_json = serde_json::to_string(&error_response)?;
                writeln!(stdout, "{}", response_json)?;
                stdout.flush()?;
                continue;
            }
        };

        let response = server.handle_request(request);
        let response_json = serde_json::to_string(&response)?;
        writeln!(stdout, "{}", response_json)?;
        stdout.flush()?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_fortunes() -> Vec<Fortune> {
        vec![
            Fortune {
                text: "Short fortune".to_string(),
                database: "test".to_string(),
            },
            Fortune {
                text: "This is a much longer fortune that exceeds the 160 character limit for short fortunes. It contains multiple sentences and goes on for quite a while to ensure it's definitely over the limit.".to_string(),
                database: "test".to_string(),
            },
            Fortune {
                text: "Offensive content".to_string(),
                database: "offensive".to_string(),
            },
            Fortune {
                text: "Star Trek fortune".to_string(),
                database: "startrek".to_string(),
            },
        ]
    }

    #[test]
    fn test_fortune_struct() {
        let fortune = Fortune {
            text: "Test fortune".to_string(),
            database: "test".to_string(),
        };
        assert_eq!(fortune.text, "Test fortune");
        assert_eq!(fortune.database, "test");
    }

    #[test]
    fn test_is_offensive_database() {
        assert!(McpServer::is_offensive_database("offensive"));
        assert!(McpServer::is_offensive_database("limerick"));
        assert!(McpServer::is_offensive_database("sex"));
        assert!(McpServer::is_offensive_database("racist"));
        assert!(McpServer::is_offensive_database("ethnic"));
        assert!(!McpServer::is_offensive_database("computers"));
        assert!(!McpServer::is_offensive_database("startrek"));
    }

    #[test]
    fn test_filter_fortunes_short_only() {
        let fortunes = create_test_fortunes();
        let mut databases = HashMap::new();
        for (idx, fortune) in fortunes.iter().enumerate() {
            databases.entry(fortune.database.clone())
                .or_insert_with(Vec::new)
                .push(idx);
        }

        let server = McpServer {
            name: "test".to_string(),
            version: "0.1.0".to_string(),
            fortunes,
            databases,
        };

        let all_indices: Vec<usize> = (0..server.fortunes.len()).collect();
        let filtered = server.filter_fortunes(&all_indices, true, true);

        // Should only include fortunes with 160 chars or less
        assert_eq!(filtered.len(), 3); // Excludes the long fortune
        assert!(server.fortunes[filtered[0]].text.len() <= 160);
    }

    #[test]
    fn test_filter_fortunes_no_offensive() {
        let fortunes = create_test_fortunes();
        let mut databases = HashMap::new();
        for (idx, fortune) in fortunes.iter().enumerate() {
            databases.entry(fortune.database.clone())
                .or_insert_with(Vec::new)
                .push(idx);
        }

        let server = McpServer {
            name: "test".to_string(),
            version: "0.1.0".to_string(),
            fortunes,
            databases,
        };

        let all_indices: Vec<usize> = (0..server.fortunes.len()).collect();
        let filtered = server.filter_fortunes(&all_indices, false, false);

        // Should exclude offensive database
        assert_eq!(filtered.len(), 3); // Excludes offensive fortune
        for &idx in &filtered {
            assert!(!McpServer::is_offensive_database(&server.fortunes[idx].database));
        }
    }

    #[test]
    fn test_database_indexing() {
        let fortunes = create_test_fortunes();
        let mut databases = HashMap::new();
        for (idx, fortune) in fortunes.iter().enumerate() {
            databases.entry(fortune.database.clone())
                .or_insert_with(Vec::new)
                .push(idx);
        }

        assert_eq!(databases.get("test").unwrap().len(), 2);
        assert_eq!(databases.get("offensive").unwrap().len(), 1);
        assert_eq!(databases.get("startrek").unwrap().len(), 1);
    }

    #[test]
    fn test_handle_initialize() {
        let fortunes = create_test_fortunes();
        let mut databases = HashMap::new();
        for (idx, fortune) in fortunes.iter().enumerate() {
            databases.entry(fortune.database.clone())
                .or_insert_with(Vec::new)
                .push(idx);
        }

        let server = McpServer {
            name: "test-server".to_string(),
            version: "1.0.0".to_string(),
            fortunes,
            databases,
        };

        let result = server.handle_initialize(None).unwrap();
        assert_eq!(result["protocolVersion"], "2024-11-05");
        assert_eq!(result["serverInfo"]["name"], "test-server");
        assert_eq!(result["serverInfo"]["version"], "1.0.0");
    }

    #[test]
    fn test_handle_tools_list() {
        let fortunes = create_test_fortunes();
        let mut databases = HashMap::new();
        for (idx, fortune) in fortunes.iter().enumerate() {
            databases.entry(fortune.database.clone())
                .or_insert_with(Vec::new)
                .push(idx);
        }

        let server = McpServer {
            name: "test".to_string(),
            version: "0.1.0".to_string(),
            fortunes,
            databases,
        };

        let result = server.handle_tools_list().unwrap();
        assert!(result["tools"].is_array());
        assert_eq!(result["tools"][0]["name"], "get_fortune");
    }

    #[test]
    fn test_get_fortune_from_database() {
        let fortunes = create_test_fortunes();
        let mut databases = HashMap::new();
        for (idx, fortune) in fortunes.iter().enumerate() {
            databases.entry(fortune.database.clone())
                .or_insert_with(Vec::new)
                .push(idx);
        }

        let server = McpServer {
            name: "test".to_string(),
            version: "0.1.0".to_string(),
            fortunes,
            databases,
        };

        let fortune = server.get_fortune_from_database("startrek", false, true).unwrap();
        assert_eq!(fortune.database, "startrek");
        assert_eq!(fortune.text, "Star Trek fortune");
    }

    #[test]
    fn test_get_fortune_from_invalid_database() {
        let fortunes = create_test_fortunes();
        let mut databases = HashMap::new();
        for (idx, fortune) in fortunes.iter().enumerate() {
            databases.entry(fortune.database.clone())
                .or_insert_with(Vec::new)
                .push(idx);
        }

        let server = McpServer {
            name: "test".to_string(),
            version: "0.1.0".to_string(),
            fortunes,
            databases,
        };

        let result = server.get_fortune_from_database("nonexistent", false, true);
        assert!(result.is_err());
    }
}
