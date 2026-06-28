use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Fact {
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub valid_from: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Contradiction {
    pub fact_a: Fact,
    pub fact_b: Fact,
    pub description: String,
}

fn neo4j_url() -> String {
    std::env::var("NEO4J_HTTP_URL").unwrap_or_else(|_| "http://localhost:7474".into())
}

fn neo4j_auth() -> Option<(String, String)> {
    let user = std::env::var("NEO4J_USER").unwrap_or_else(|_| "neo4j".into());
    let pass = std::env::var("NEO4J_PASSWORD").ok()?;
    Some((user, pass))
}

async fn cypher_query(statement: &str, params: &serde_json::Value) -> Result<serde_json::Value> {
    let url = format!("{}/db/neo4j/tx/commit", neo4j_url());
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "statements": [{
            "statement": statement,
            "parameters": params
        }]
    });
    let mut req = client.post(&url).json(&body);
    if let Some((user, pass)) = neo4j_auth() {
        req = req.basic_auth(user, Some(pass));
    }
    let resp = req.send().await.context("Neo4j HTTP request failed")?;
    let result: serde_json::Value = resp.json().await?;
    Ok(result)
}

pub async fn add_fact(
    subject: &str,
    predicate: &str,
    object: &str,
    valid_from: &str,
) -> Result<()> {
    let cypher = "MERGE (s:Entity {name: $subject}) \
                  MERGE (o:Entity {name: $object}) \
                  CREATE (s)-[r:RELATES {predicate: $predicate, valid_from: $valid_from}]->(o) \
                  RETURN id(r)";
    let params = serde_json::json!({
        "subject": subject,
        "predicate": predicate,
        "object": object,
        "valid_from": valid_from,
    });
    let result = cypher_query(cypher, &params).await?;
    if let Some(errors) = result["errors"].as_array() {
        if !errors.is_empty() {
            anyhow::bail!("Neo4j errors: {:?}", errors);
        }
    }
    Ok(())
}

pub async fn query_facts(topic: &str) -> Result<Vec<Fact>> {
    let cypher = "MATCH (s:Entity)-[r:RELATES]->(o:Entity) \
                  WHERE s.name CONTAINS $topic OR o.name CONTAINS $topic \
                  RETURN s.name AS subject, r.predicate AS predicate, o.name AS object, r.valid_from AS valid_from \
                  ORDER BY r.valid_from DESC LIMIT 50";
    let params = serde_json::json!({"topic": topic});
    let result = cypher_query(cypher, &params).await?;

    let mut facts = Vec::new();
    if let Some(rows) = result["results"][0]["data"].as_array() {
        for row in rows {
            if let Some(vals) = row["row"].as_array() {
                facts.push(Fact {
                    subject: vals[0].as_str().unwrap_or("").to_string(),
                    predicate: vals[1].as_str().unwrap_or("").to_string(),
                    object: vals[2].as_str().unwrap_or("").to_string(),
                    valid_from: vals[3].as_str().unwrap_or("").to_string(),
                });
            }
        }
    }
    Ok(facts)
}

pub async fn find_contradictions() -> Result<Vec<Contradiction>> {
    let cypher = "MATCH (s:Entity)-[r1:RELATES]->(o1:Entity), \
                        (s)-[r2:RELATES]->(o2:Entity) \
                  WHERE r1.predicate = r2.predicate AND o1 <> o2 AND id(r1) < id(r2) \
                  RETURN s.name, r1.predicate, o1.name, r1.valid_from, o2.name, r2.valid_from \
                  LIMIT 20";
    let result = cypher_query(cypher, &serde_json::json!({})).await?;

    let mut contradictions = Vec::new();
    if let Some(rows) = result["results"][0]["data"].as_array() {
        for row in rows {
            if let Some(vals) = row["row"].as_array() {
                contradictions.push(Contradiction {
                    fact_a: Fact {
                        subject: vals[0].as_str().unwrap_or("").to_string(),
                        predicate: vals[1].as_str().unwrap_or("").to_string(),
                        object: vals[2].as_str().unwrap_or("").to_string(),
                        valid_from: vals[3].as_str().unwrap_or("").to_string(),
                    },
                    fact_b: Fact {
                        subject: vals[0].as_str().unwrap_or("").to_string(),
                        predicate: vals[1].as_str().unwrap_or("").to_string(),
                        object: vals[4].as_str().unwrap_or("").to_string(),
                        valid_from: vals[5].as_str().unwrap_or("").to_string(),
                    },
                    description: format!(
                        "Conflicting objects for {} -[{}]->: {} vs {}",
                        vals[0].as_str().unwrap_or(""),
                        vals[1].as_str().unwrap_or(""),
                        vals[2].as_str().unwrap_or(""),
                        vals[4].as_str().unwrap_or("")
                    ),
                });
            }
        }
    }
    Ok(contradictions)
}
