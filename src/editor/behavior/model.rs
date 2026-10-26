use super::{Command, ObjectId, Value, json};
use crate::editor::document::Document;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Block {
    Move,
    Wait,
    Investigate,
    Fetch,
    Use,
    Follow,
    Routine,
    Patrol,
    Hear,
    See,
}
impl Block {
    pub(crate) const ALL: [Self; 10] = [
        Self::Routine,
        Self::Move,
        Self::Wait,
        Self::Investigate,
        Self::Fetch,
        Self::Use,
        Self::Follow,
        Self::Patrol,
        Self::Hear,
        Self::See,
    ];
    pub(crate) const fn title(self) -> &'static str {
        match self {
            Self::Move => "Move to location",
            Self::Wait => "Wait",
            Self::Investigate => "Investigate / search",
            Self::Fetch => "Fetch / deliver",
            Self::Use => "Use environment",
            Self::Follow => "Follow / escort",
            Self::Routine => "Ordered routine",
            Self::Patrol => "Patrol",
            Self::Hear => "When hearing",
            Self::See => "When seeing",
        }
    }
    fn value(self) -> Value {
        let contact = json!({"target":"","approach":[0.0_f32,0.0_f32,0.0_f32],"contact":[0.0_f32,1.0_f32,0.0_f32],"reach":1.5_f32,"yaw_degrees":0.0_f32,"kind":if matches!(self, Self::Fetch) { "shelf" } else { "interact" },"contact_seconds":0.35_f32,"hold_seconds":0.0_f32});
        match self {
            Self::Move => json!({"task":"move","point":[0.0_f32,0.0_f32,0.0_f32],"speed":1.2_f32}),
            Self::Wait => json!({"task":"wait","seconds":1.0_f32}),
            Self::Investigate => {
                json!({"task":"investigate","point":[0.0_f32,0.0_f32,0.0_f32],"seconds":8.0_f32,"radius":3.0_f32})
            }
            Self::Fetch => {
                json!({"task":"fetch","item":{"match":"id","value":""},"destination":contact,"search_radius":30.0_f32})
            }
            Self::Use => json!({"task":"use","point":contact}),
            Self::Follow => {
                json!({"task":"follow","target":"player","distance":1.6_f32,"memory_seconds":12.0_f32})
            }
            Self::Routine => json!({"task":"routine","steps":[],"looped":false}),
            Self::Patrol => {
                json!({"task":"patrol","points":[[0.0_f32,0.0_f32,0.0_f32]],"speed":1.2_f32,"memory_seconds":12.0_f32,"track_player":false,"decision_seconds":0.8_f32})
            }
            Self::Hear => {
                json!({"filter":{"sense":"hear","categories":[],"radius":12.0_f32},"response":{"response":"investigate","seconds":8.0_f32,"radius":3.0_f32}})
            }
            Self::See => {
                json!({"filter":{"sense":"see","player":true,"entities":false,"objects":[],"object_types":[],"range":20.0_f32,"fov_degrees":100.0_f32},"response":{"response":"investigate","seconds":8.0_f32,"radius":3.0_f32}})
            }
        }
    }
}
pub struct Row {
    pub path: String,
    pub label: String,
    pub depth: usize,
}
pub struct Field {
    pub path: String,
    pub label: String,
    pub value: Value,
}
pub struct Choices {
    pub path: String,
    pub values: Vec<String>,
    pub multiple: bool,
}
pub struct Draft {
    pub id: ObjectId,
    pub root: std::path::PathBuf,
    pub level: String,
    pub expected: Value,
    pub plan: Value,
    pub original_plan: Value,
    pub selected: Option<String>,
    pub row_page: usize,
    pub field_page: usize,
    pub edit: Option<(String, String)>,
    pub choices: Option<Choices>,
    pub locations: bool,
    pub runtime: Option<Vec<String>>,
    pub runtime_page: usize,
    pub drag: Option<(Command, [f32; 2])>,
}
impl Draft {
    pub(crate) fn new(doc: &Document, id: ObjectId) -> Result<Self, String> {
        Ok(Self {
            id,
            root: doc.root().to_path_buf(),
            level: doc.level_id().into(),
            expected: doc.raw().clone(),
            plan: doc.behavior_value(id)?,
            original_plan: doc.behavior_value(id)?,
            selected: None,
            row_page: 0,
            field_page: 0,
            edit: None,
            choices: None,
            locations: false,
            drag: None,
            runtime: None,
            runtime_page: 0,
        })
    }
    pub(crate) fn toggle_alerts(&mut self) -> Result<(), String> {
        let enabled = self
            .plan
            .get("receive_alerts")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        *self
            .plan
            .get_mut("receive_alerts")
            .ok_or("Missing alert option")? = json!(!enabled);
        Ok(())
    }
    pub(crate) fn page_fields(&mut self, doc: &Document, forward: bool) {
        let count = if self.locations {
            doc.level().rooms.len()
        } else {
            self.choices
                .as_ref()
                .map_or_else(|| self.fields().len(), |choices| choices.values.len())
        };
        self.field_page = if forward {
            self.field_page
                .saturating_add(1)
                .min(count.saturating_sub(1) / 6)
        } else {
            self.field_page.saturating_sub(1)
        };
    }
    pub(crate) fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        for (index, request) in self
            .plan
            .get("tasks")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
        {
            let path = format!("/tasks/{index}");
            append_rows(&mut rows, &path, request, 0);
        }
        for (index, sense) in self
            .plan
            .get("senses")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
        {
            let label = format!(
                "IF {} -> {}",
                sense
                    .pointer("/filter/sense")
                    .and_then(Value::as_str)
                    .unwrap_or("?"),
                sense
                    .pointer("/response/response")
                    .and_then(Value::as_str)
                    .unwrap_or("?")
            );
            let path = format!("/senses/{index}");
            rows.push(Row {
                path: path.clone(),
                label,
                depth: 0,
            });
            if let Some(request) = sense.pointer("/response/request") {
                append_rows(&mut rows, &format!("{path}/response/request"), request, 1);
            }
        }
        rows
    }
    pub(crate) fn select(&mut self, index: usize) -> Result<(), String> {
        let rows = self.rows();
        self.selected = Some(
            rows.get(index)
                .ok_or("Block no longer exists")?
                .path
                .clone(),
        );
        self.field_page = 0;
        self.choices = None;
        Ok(())
    }
    pub(crate) fn add(&mut self, kind: Block, target: Option<usize>) -> Result<(), String> {
        if self.rows().len() >= 128 {
            return Err("Visual plan limit: 128 blocks".into());
        }
        if let Some(index) = target {
            self.select(index)?;
        }
        let path = if matches!(kind, Block::Hear | Block::See) {
            "/senses".to_string()
        } else {
            self.selected
                .as_deref()
                .and_then(|selected| self.plan.pointer(selected).map(|v| (selected, v)))
                .and_then(|(selected, value)| {
                    if value.get("task").and_then(Value::as_str) == Some("routine") {
                        Some(format!("{selected}/steps"))
                    } else if value.pointer("/definition/task").and_then(Value::as_str)
                        == Some("routine")
                    {
                        Some(format!("{selected}/definition/steps"))
                    } else if selected.contains("/steps/") {
                        selected
                            .rsplit_once('/')
                            .map(|(parent, _)| parent.to_string())
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| "/tasks".into())
        };
        let entries = self
            .plan
            .pointer_mut(&path)
            .and_then(Value::as_array_mut)
            .ok_or("Select a routine or startup task")?;
        let bound = if path == "/tasks" || path == "/senses" {
            16
        } else {
            32
        };
        if entries.len() >= bound {
            return Err(format!("This container allows at most {bound} blocks"));
        }
        let value = if path == "/tasks" {
            let key = (1..=1000u16)
                .map(|i| format!("editor_task_{i}"))
                .find(|key| {
                    !entries
                        .iter()
                        .any(|v| v.get("key").and_then(Value::as_str) == Some(key))
                })
                .ok_or("No unique task key")?;
            json!({"key":key,"priority":20_u8,"definition":kind.value()})
        } else {
            kind.value()
        };
        let index = entries.len();
        entries.push(value);
        self.selected = Some(format!("{path}/{index}"));
        self.field_page = 0;
        Ok(())
    }
    pub(crate) fn remove(&mut self) -> Result<(), String> {
        let selected = self.selected.as_deref().ok_or("Select a block")?;
        let (parent, slot) = selected.rsplit_once('/').ok_or("Invalid block")?;
        let index = slot
            .parse::<usize>()
            .map_err(|_error| "Remove the whole perception rule")?;
        let entries = self
            .plan
            .pointer_mut(parent)
            .and_then(Value::as_array_mut)
            .ok_or("Remove the whole perception rule")?;
        if index >= entries.len() {
            return Err("Block no longer exists".into());
        }
        drop(entries.remove(index));
        self.selected = None;
        Ok(())
    }
    pub(crate) fn reorder(&mut self, from: usize, to: Option<usize>) -> Result<(), String> {
        let rows = self.rows();
        let Some(target) = to.and_then(|index| rows.get(index)) else {
            return Ok(());
        };
        let source = rows.get(from).ok_or("Block no longer exists")?;
        let (parent, slot) = source.path.rsplit_once('/').ok_or("Invalid source")?;
        let (destination, ordinal) = target.path.rsplit_once('/').ok_or("Invalid target")?;
        if parent != destination {
            return Err("Reorder within the same routine or rule list; use the palette to add inside a different routine.".into());
        }
        let a = slot
            .parse::<usize>()
            .map_err(|_error| "Reorder the containing rule")?;
        let b = ordinal
            .parse::<usize>()
            .map_err(|_error| "Reorder the containing rule")?;
        let entries = self
            .plan
            .pointer_mut(parent)
            .and_then(Value::as_array_mut)
            .ok_or("Invalid block list")?;
        if a >= entries.len() || b >= entries.len() {
            return Err("Block no longer exists".into());
        }
        let value = entries.remove(a);
        entries.insert(b, value);
        self.selected = Some(format!("{parent}/{b}"));
        Ok(())
    }
    pub(crate) fn fields(&self) -> Vec<Field> {
        let mut fields = Vec::new();
        if let Some(path) = &self.selected
            && let Some(value) = self.plan.pointer(path)
        {
            append_fields(&mut fields, path, "", value);
        }
        fields
    }
    pub(crate) fn finish_field(&mut self) -> Result<(), String> {
        let Some((path, text)) = &self.edit else {
            return Ok(());
        };
        let old = self.plan.pointer(path).ok_or("Field no longer exists")?;
        let value = if old.is_number() {
            let number: Value = serde_json::from_str(text)
                .map_err(|_error| format!("{path}: enter a finite number"))?;
            if !number.is_number() {
                return Err(format!("{path}: enter a finite number"));
            }
            number
        } else {
            json!(text)
        };
        *self.plan.pointer_mut(path).ok_or("Missing field")? = value;
        self.edit = None;
        Ok(())
    }
    pub(crate) fn open_field(&mut self, doc: &Document, index: usize) -> Result<(), String> {
        let fields = self.fields();
        let field = fields.get(index).ok_or("Field no longer exists")?;
        if let Some(value) = field.value.as_bool() {
            *self.plan.pointer_mut(&field.path).ok_or("Missing field")? = json!(!value);
            return Ok(());
        }
        let key = field.path.rsplit('/').next().unwrap_or_default();
        let mut choices = match key {
            "categories" => [
                "Any", "movement", "door", "interact", "spawn", "impact", "sound", "alert",
            ]
            .map(str::to_owned)
            .to_vec(),
            "kind" => ["interact", "door", "chair", "switch", "shelf"]
                .map(str::to_owned)
                .to_vec(),
            "match" => ["id", "object_type"].map(str::to_owned).to_vec(),
            "target" | "objects" => {
                let mut ids = doc.level().prop_instance_ids();
                ids.extend(doc.level().door_instance_ids());
                ids.extend(doc.level().light_instance_ids());
                if key == "target" {
                    ids.insert(0, "player".into());
                }
                ids
            }
            "object_types" => doc.level().props.iter().map(|p| p.model.clone()).collect(),
            "value" => {
                let parent = field.path.rsplit_once('/').map_or("", |(p, _)| p);
                if self
                    .plan
                    .pointer(&format!("{parent}/match"))
                    .and_then(Value::as_str)
                    == Some("object_type")
                {
                    doc.level().props.iter().map(|p| p.model.clone()).collect()
                } else {
                    doc.level().prop_instance_ids()
                }
            }
            _ => Vec::new(),
        };
        if field.value.is_array() && key != "categories" {
            choices.insert(0, "None".into());
        }
        if choices.is_empty() {
            self.edit = Some((
                field.path.clone(),
                field
                    .value
                    .as_str()
                    .map_or_else(|| field.value.to_string(), str::to_owned),
            ));
        } else {
            let mut unique = Vec::new();
            for value in choices {
                if !unique.contains(&value) {
                    unique.push(value);
                }
            }
            self.field_page = 0;
            self.choices = Some(Choices {
                path: field.path.clone(),
                values: unique,
                multiple: field.value.is_array(),
            });
        }
        Ok(())
    }
    pub(crate) fn choose(&mut self, index: usize) -> Result<(), String> {
        let choice = self.choices.as_ref().ok_or("Open a choice field")?;
        let selected = choice
            .values
            .get(index)
            .ok_or("Choice no longer exists")?
            .clone();
        let value = self.plan.pointer_mut(&choice.path).ok_or("Missing field")?;
        if choice.multiple {
            let values = value.as_array_mut().ok_or("Invalid selection list")?;
            if matches!(selected.as_str(), "Any" | "None") {
                values.clear();
            } else if let Some(selected_index) =
                values.iter().position(|v| v.as_str() == Some(&selected))
            {
                drop(values.remove(selected_index));
            } else {
                values.push(json!(selected));
            }
        } else {
            *value = json!(selected);
            let field_path = choice.path.clone();
            self.choices = None;
            self.field_page = self
                .fields()
                .iter()
                .position(|field| field.path == field_path)
                .unwrap_or(0)
                / 6;
        }
        Ok(())
    }
    pub(crate) fn response(&mut self, kind: u8) -> Result<(), String> {
        let selected = self.selected.as_deref().ok_or("Select a perception rule")?;
        let rule = self
            .plan
            .pointer_mut(selected)
            .and_then(Value::as_object_mut)
            .ok_or("Select a perception rule")?;
        if !rule.contains_key("filter") {
            return Err("Select the IF hearing/sight block first".into());
        }
        let response = match kind {
            0 => json!({"response":"investigate","seconds":8.0_f32,"radius":3.0_f32}),
            1 => {
                json!({"response":"task","request":{"key":"perception_response","priority":60_u8,"definition":Block::Routine.value()}})
            }
            2 => json!({"response":"alert","radius":20.0_f32,"hops":1_u8}),
            _ => return Err("Unknown response".into()),
        };
        drop(rule.insert("response".into(), response));
        Ok(())
    }
    pub(crate) fn location(&mut self, doc: &Document, index: usize) -> Result<(), String> {
        let room = doc
            .level()
            .rooms
            .get(index)
            .ok_or("Room no longer exists")?;
        let point = json!([
            f32::mul_add(room.width, 0.5_f32, room.x),
            room.floor_y,
            f32::mul_add(room.depth, 0.5_f32, room.z)
        ]);
        let selected = self.selected.as_deref().ok_or("Select an action block")?;
        let value = self
            .plan
            .pointer_mut(selected)
            .ok_or("Block no longer exists")?;
        let task = if value.get("definition").is_some() {
            value.get_mut("definition").ok_or("Missing definition")?
        } else {
            value
        };
        match task.get("task").and_then(Value::as_str) {
            Some("move"|"investigate")=>{*task.get_mut("point").ok_or("Missing point")?=point;},
            Some("patrol")=>{let points=task.get_mut("points").and_then(Value::as_array_mut).ok_or("Missing patrol")?;if points.len()>=32{return Err("Patrol has 32 points".into());}points.push(point);},
            _=>return Err("Location is available for Move, Investigate and Patrol. Contact/approach points use precise fields.".into()),
        }
        Ok(())
    }
}
fn append_rows(rows: &mut Vec<Row>, path: &str, value: &Value, depth: usize) {
    if depth > 4 || rows.len() >= 128 {
        return;
    }
    let definition = value.get("definition").unwrap_or(value);
    let kind = definition
        .get("task")
        .and_then(Value::as_str)
        .unwrap_or("?");
    let label = value
        .get("key")
        .and_then(Value::as_str)
        .map_or_else(|| kind.to_string(), |key| format!("{key}: {kind}"));
    rows.push(Row {
        path: path.into(),
        label,
        depth,
    });
    let base = if value.get("definition").is_some() {
        format!("{path}/definition")
    } else {
        path.into()
    };
    for (index, step) in definition
        .get("steps")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        append_rows(
            rows,
            &format!("{base}/steps/{index}"),
            step,
            depth.saturating_add(1),
        );
    }
}
fn append_fields(fields: &mut Vec<Field>, path: &str, label: &str, value: &Value) {
    if fields.len() >= 128 {
        return;
    }
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if matches!(key.as_str(), "task" | "sense" | "response" | "steps")
                    && !child.is_object()
                {
                    continue;
                }
                if key == "request" {
                    continue;
                }
                let caption = if label.is_empty() {
                    key.clone()
                } else {
                    format!("{label} / {key}")
                };
                append_fields(fields, &format!("{path}/{key}"), &caption, child);
            }
        }
        Value::Array(values) if values.iter().all(Value::is_string) => fields.push(Field {
            path: path.into(),
            label: label.into(),
            value: value.clone(),
        }),
        Value::Array(values) => {
            for (index, child) in values.iter().enumerate() {
                let axis = ["X", "Y", "Z"].get(index).copied().unwrap_or("point");
                append_fields(
                    fields,
                    &format!("{path}/{index}"),
                    &format!("{label} {axis}"),
                    child,
                );
            }
        }
        Value::Null => {}
        Value::Bool(_) | Value::Number(_) | Value::String(_) => fields.push(Field {
            path: path.into(),
            label: label.into(),
            value: value.clone(),
        }),
    }
}
