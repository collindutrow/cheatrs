use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use web_sys::{KeyboardEvent, window};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"])]
    async fn invoke(cmd: &str, args: JsValue) -> JsValue;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct CheatItem {
    keys: Vec<String>,
    desc: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct CheatSection {
    title: String,
    items: Vec<CheatItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct CheatSheet {
    id: String,
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    hint: Option<String>,
    #[serde(default)]
    processes: Vec<String>,
    sections: Vec<CheatSection>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct AppConfig {
    #[serde(default)]
    last_cheatsheet_id: Option<String>,
    #[serde(default)]
    last_cheatsheet_per_process: std::collections::HashMap<String, String>,
    #[serde(default)]
    search_all_for_process: bool,
}

fn fuzzy_score(query: &str, text: &str) -> f64 {
    let query = query.to_lowercase();
    let text = text.to_lowercase();

    let mut qi = 0;
    let mut ti = 0;
    let mut score = 0.0;

    let query_chars: Vec<char> = query.chars().collect();
    let text_chars: Vec<char> = text.chars().collect();

    while qi < query_chars.len() {
        let qc = query_chars[qi];
        if qc == ' ' {
            qi += 1;
            score += 0.5;
            continue;
        }

        if let Some(idx) = text_chars[ti..].iter().position(|&c| c == qc) {
            let actual_idx = ti + idx;
            score += 1.0;
            if actual_idx == ti {
                score += 1.0;
            }
            if actual_idx == 0
                || text_chars[actual_idx - 1].is_whitespace()
                || !text_chars[actual_idx - 1].is_alphanumeric()
            {
                score += 0.5;
            }
            ti = actual_idx + 1;
        } else {
            return 0.0;
        }
        qi += 1;
    }

    score / (1.0 + text_chars.len() as f64 / 120.0)
}

async fn load_sheets() -> Vec<CheatSheet> {
    // Call the Tauri command to load cheatsheets from filesystem
    match invoke("load_cheatsheets", JsValue::NULL).await {
        result if !result.is_undefined() && !result.is_null() => {
            match serde_wasm_bindgen::from_value::<Vec<CheatSheet>>(result) {
                Ok(sheets) => sheets,
                Err(e) => {
                    log(&format!("Failed to parse cheatsheets: {:?}", e));
                    Vec::new()
                }
            }
        }
        _ => {
            log("Failed to load cheatsheets from Tauri command");
            Vec::new()
        }
    }
}

#[derive(Clone, PartialEq)]
struct IndexEntryWithTags {
    sheet_id: String,
    section_index: usize,
    item_index: usize,
    text: String,
    tags_text: String,
}

fn build_index(
    sheets: &[CheatSheet],
    process_filter: Option<String>,
    search_all: bool,
) -> Vec<IndexEntryWithTags> {
    let mut index = Vec::new();

    let filtered_sheets: Vec<&CheatSheet> = if let Some(ref process) = process_filter {
        if search_all {
            sheets
                .iter()
                .filter(|sheet| {
                    sheet.processes.is_empty()
                        || sheet
                            .processes
                            .iter()
                            .any(|p| p.to_lowercase() == process.to_lowercase())
                })
                .collect()
        } else {
            sheets.iter().collect()
        }
    } else {
        sheets.iter().collect()
    };

    for sheet in filtered_sheets {
        for (section_index, section) in sheet.sections.iter().enumerate() {
            for (item_index, item) in section.items.iter().enumerate() {
                let mut parts = Vec::new();
                parts.extend(item.keys.iter().cloned());
                parts.push(item.desc.clone());
                parts.extend(item.tags.iter().cloned());
                if let Some(hint) = &item.hint {
                    parts.push(hint.clone());
                }
                parts.push(sheet.name.clone());
                parts.push(sheet.description.clone());

                let tags_text = item.tags.join(" ").to_lowercase();

                index.push(IndexEntryWithTags {
                    sheet_id: sheet.id.clone(),
                    section_index,
                    item_index,
                    text: parts.join(" ").to_lowercase(),
                    tags_text,
                });
            }
        }
    }

    index
}

#[component]
pub fn App() -> impl IntoView {
    let (sheets, set_sheets) = signal(Vec::<CheatSheet>::new());
    let (current_sheet_id, set_current_sheet_id) = signal(String::new());
    let (search_query, set_search_query) = signal(String::new());
    let (show_tags, set_show_tags) = signal(false);
    let (current_process, set_current_process) = signal(Option::<String>::None);
    let (search_all_for_process, set_search_all_for_process) = signal(true);

    // Load sheets and initial config on mount
    Effect::new(move || {
        leptos::task::spawn_local(async move {
            // Load current process
            let process = invoke("get_current_process", JsValue::NULL).await;
            if !process.is_undefined() && !process.is_null() {
                if let Ok(p) = serde_wasm_bindgen::from_value::<String>(process) {
                    set_current_process.set(Some(p));
                }
            }

            // Load config
            let config_result = invoke("get_config", JsValue::NULL).await;
            if !config_result.is_undefined() && !config_result.is_null() {
                if let Ok(config) = serde_wasm_bindgen::from_value::<AppConfig>(config_result) {
                    set_search_all_for_process.set(config.search_all_for_process);
                }
            }

            // Load sheets
            let loaded_sheets = load_sheets().await;

            // Get initial sheet ID
            let initial_id_result = invoke("get_initial_sheet_id", JsValue::NULL).await;
            let initial_id = if !initial_id_result.is_undefined() && !initial_id_result.is_null() {
                serde_wasm_bindgen::from_value::<Option<String>>(initial_id_result)
                    .ok()
                    .flatten()
            } else {
                None
            };

            // Set current sheet
            if let Some(id) = initial_id {
                if loaded_sheets.iter().any(|s| s.id == id) {
                    set_current_sheet_id.set(id);
                } else if let Some(first_sheet) = loaded_sheets.first() {
                    set_current_sheet_id.set(first_sheet.id.clone());
                }
            } else if let Some(first_sheet) = loaded_sheets.first() {
                set_current_sheet_id.set(first_sheet.id.clone());
            }

            set_sheets.set(loaded_sheets);
        });
    });

    // Update current process every time window becomes visible
    Effect::new(move || {
        use wasm_bindgen::JsCast;

        #[wasm_bindgen]
        extern "C" {
            #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"])]
            async fn listen(event: &str, handler: &js_sys::Function) -> JsValue;
        }

        leptos::task::spawn_local(async move {
            let handler = Closure::<dyn Fn(JsValue)>::new(move |_event: JsValue| {
                // Fetch the captured process from backend (captured before window was shown)
                leptos::task::spawn_local(async move {
                    let process = invoke("get_current_process", JsValue::NULL).await;
                    if !process.is_undefined() && !process.is_null() {
                        if let Ok(p) = serde_wasm_bindgen::from_value::<String>(process) {
                            set_current_process.set(Some(p.clone()));

                            // Check if current cheatsheet fits the process
                            let current_id = current_sheet_id.get();
                            let all_sheets = sheets.get();

                            // Find current sheet
                            let current_sheet_matches = all_sheets.iter().find(|s| s.id == current_id)
                                .map(|sheet| {
                                    sheet.processes.is_empty() ||
                                    sheet.processes.iter().any(|proc| proc.to_lowercase() == p.to_lowercase())
                                })
                                .unwrap_or(false);

                            // If current sheet doesn't match, try to switch
                            if !current_sheet_matches {
                                // First, check if there's a saved preference for this process
                                use serde_wasm_bindgen::to_value;
                                let args = serde_json::json!({
                                    "processName": p
                                });

                                let mut target_sheet_id: Option<String> = None;

                                if let Ok(js_args) = to_value(&args) {
                                    let saved_sheet = invoke("get_sheet_for_process", js_args).await;
                                    if !saved_sheet.is_undefined() && !saved_sheet.is_null() {
                                        if let Ok(sheet_id) = serde_wasm_bindgen::from_value::<Option<String>>(saved_sheet) {
                                            // Verify the saved sheet matches the process
                                            if let Some(ref saved_id) = sheet_id {
                                                if let Some(sheet) = all_sheets.iter().find(|s| &s.id == saved_id) {
                                                    if sheet.processes.iter().any(|proc| proc.to_lowercase() == p.to_lowercase()) {
                                                        target_sheet_id = Some(saved_id.clone());
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                // If no saved preference or saved sheet doesn't match, find first matching sheet
                                if target_sheet_id.is_none() {
                                    if let Some(matching_sheet) = all_sheets.iter().find(|sheet| {
                                        !sheet.processes.is_empty() &&
                                        sheet.processes.iter().any(|proc| proc.to_lowercase() == p.to_lowercase())
                                    }) {
                                        target_sheet_id = Some(matching_sheet.id.clone());
                                    }
                                }

                                // Switch to the target sheet if found
                                if let Some(id) = target_sheet_id {
                                    set_current_sheet_id.set(id);
                                }
                            }
                        }
                    }
                });
            });

            // Listen for custom event emitted when process is captured
            let _ = listen("process-captured", handler.as_ref().unchecked_ref()).await;
            handler.forget();
        });
    });

    // Save last cheatsheet when it changes
    Effect::new(move || {
        let sheet_id = current_sheet_id.get();
        let process = current_process.get();
        if !sheet_id.is_empty() {
            leptos::task::spawn_local(async move {
                use serde_wasm_bindgen::to_value;

                // Get the current sheet's processes list
                let all_sheets = sheets.get();
                let sheet_processes = all_sheets
                    .iter()
                    .find(|s| s.id == sheet_id)
                    .map(|s| s.processes.clone())
                    .unwrap_or_default();

                let args = serde_json::json!({
                    "sheetId": sheet_id,
                    "processName": process,
                    "sheetProcesses": sheet_processes
                });
                if let Ok(js_args) = to_value(&args) {
                    let _ = invoke("update_last_cheatsheet", js_args).await;
                }
            });
        }
    });

    // Build search index with process filtering
    let index = Memo::new(move |_| {
        let process = if search_all_for_process.get() {
            current_process.get()
        } else {
            None
        };
        build_index(&sheets.get(), process, search_all_for_process.get())
    });

    // Get current sheet
    let current_sheet = Memo::new(move |_| {
        let sheet_id = current_sheet_id.get();
        sheets.get().into_iter().find(|s| s.id == sheet_id)
    });

    // Filter sheets for dropdown based on process
    let filtered_sheets = Memo::new(move |_| {
        let all_sheets = sheets.get();
        if search_all_for_process.get() {
            if let Some(ref process) = current_process.get() {
                all_sheets
                    .into_iter()
                    .filter(|sheet| {
                        sheet.processes.is_empty()
                            || sheet
                                .processes
                                .iter()
                                .any(|p| p.to_lowercase() == process.to_lowercase())
                    })
                    .collect::<Vec<_>>()
            } else {
                all_sheets
            }
        } else {
            all_sheets
        }
    });

    // Handle keyboard events
    Effect::new(move |_| {
        use wasm_bindgen::JsCast;

        let handler = Closure::<dyn Fn(web_sys::Event)>::new(move |event: web_sys::Event| {
            let e: KeyboardEvent = event.dyn_into().unwrap();
            let target = e.target();
            let is_input = if let Some(element) =
                target.and_then(|t| t.dyn_into::<web_sys::HtmlElement>().ok())
            {
                let tag = element.tag_name().to_lowercase();
                tag == "input" || tag == "textarea" || element.is_content_editable()
            } else {
                false
            };

            // Alt + Arrow keys to switch sheets
            if e.alt_key() && !e.ctrl_key() && !e.meta_key() && !e.shift_key() {
                if e.key() == "ArrowUp" || e.key() == "ArrowDown" {
                    let sheets_list = filtered_sheets.get();
                    let ids: Vec<String> = sheets_list.iter().map(|s| s.id.clone()).collect();
                    let current = current_sheet_id.get();
                    if let Some(idx) = ids.iter().position(|id| id == &current) {
                        let dir = if e.key() == "ArrowDown" {
                            1
                        } else {
                            ids.len() - 1
                        };
                        let next = (idx + dir) % ids.len();
                        set_current_sheet_id.set(ids[next].clone());
                        e.prevent_default();
                    }
                    return;
                }
            }

            // Escape to clear to close window
            if e.key() == "Escape" {
                // Close the window
                leptos::task::spawn_local(async move {
                    let _ = invoke("close_window", JsValue::NULL).await;
                });
                e.prevent_default();
                return;
            }

            // Focus search on any key press
            if !is_input && !e.meta_key() && !e.ctrl_key() && !e.alt_key() {
                if e.key().len() == 1
                    && e.key()
                        .chars()
                        .all(|c| c.is_alphanumeric() || c.is_whitespace())
                {
                    if let Some(document) = window().and_then(|w| w.document()) {
                        if let Some(input) = document.get_element_by_id("search-input") {
                            if let Ok(input) = input.dyn_into::<web_sys::HtmlInputElement>() {
                                let _ = input.focus();
                            }
                        }
                    }
                }
            }
        });

        if let Some(document) = window().and_then(|w| w.document()) {
            let _ = document
                .add_event_listener_with_callback("keydown", handler.as_ref().unchecked_ref());
        }
        handler.forget();
    });

    let close_window = move |_| {
        use wasm_bindgen::JsValue;
        leptos::task::spawn_local(async move {
            let args = JsValue::NULL;
            let _ = invoke("close_window", args).await;
        });
    };

    view! {
        <>
            <div class="drag-region"></div>
            <div class="window-controls">
                <button class="close-btn" on:click=close_window aria-label="Close">
                    "✕"
                </button>
            </div>
            <main class="container">
            <div class="search-controls-bar">
                <select
                    id="sheet-select"
                    on:change=move |ev| {
                        set_current_sheet_id.set(event_target_value(&ev));
                    }
                    prop:value=move || current_sheet_id.get()
                >
                    {move || filtered_sheets.get().iter().map(|sheet| {
                        let id = sheet.id.clone();
                        let name = sheet.name.clone();
                        view! {
                            <option value=id.clone()>{name}</option>
                        }
                    }).collect::<Vec<_>>()}
                </select>

                <input
                    id="search-input"
                    type="search"
                    autocomplete="off"
                    spellcheck="false"
                    placeholder="Search cheatsheets…"
                    on:input=move |ev| {
                        set_search_query.set(event_target_value(&ev));
                    }
                    prop:value=move || search_query.get()
                />

                <div class="toggle-stack">
                    <label>
                        <input
                            type="checkbox"
                            role="switch"
                            id="toggle-tags"
                            on:change=move |ev| {
                                set_show_tags.set(event_target_checked(&ev));
                            }
                            prop:checked=move || show_tags.get()
                        />
                        "Show tags"
                    </label>

                    <label>
                        <input
                            type="checkbox"
                            role="switch"
                            id="toggle-process-filter"
                            on:change=move |ev| {
                                let checked = event_target_checked(&ev);
                                set_search_all_for_process.set(checked);
                                leptos::task::spawn_local(async move {
                                    let _ = invoke("toggle_search_all_for_process", JsValue::NULL).await;
                                });
                            }
                            prop:checked=move || search_all_for_process.get()
                        />
                        {move || {
                            if let Some(ref proc) = current_process.get() {
                                format!("Filter for {}", proc)
                            } else {
                                "Filter for process".to_string()
                            }
                        }}
                    </label>
                </div>
            </div>

            {move || {
                current_sheet.get().map(|sheet| {
                    let query = search_query.get();
                    let total_items: usize = sheet.sections.iter().map(|s| s.items.len()).sum();

                    if query.trim().is_empty() {
                        view! {
                            <div>
                                {sheet.hint.clone().map(|hint| view! {
                                    <p class="sheet-hint">{hint}</p>
                                })}
                                <p class="results-meta">
                                    {format!("{} entries · Sheet: {}", total_items, sheet.name)}
                                </p>
                                <div class="sections-grid">
                                        {sheet.sections.iter().map(|section| {
                                            let section_title = section.title.clone();
                                            let section_items = section.items.clone();
                                            view! {
                                                <article>
                                                    <header><h3>{section_title}</h3></header>
                                                    {section_items.iter().map(|item| {
                                                        let item_keys = item.keys.clone();
                                                        let item_desc = item.desc.clone();
                                                        let item_tags = item.tags.clone();
                                                        let item_hint = item.hint.clone();
                                                        view! {
                                                            <div class="cheat-item">
                                                                <div class="key-chips">
                                                                    {item_keys.iter().map(|key| {
                                                                        let k = key.clone();
                                                                        view! { <code class="key-chip">{k}</code> }
                                                                    }).collect::<Vec<_>>()}
                                                                </div>
                                                                <div>{item_desc}</div>
                                                                {move || {
                                                                    if show_tags.get() && !item_tags.is_empty() {
                                                                        view! {
                                                                            <div class="tags">
                                                                                {item_tags.iter().map(|tag| {
                                                                                    let t = tag.clone();
                                                                                    view! {
                                                                                        <small
                                                                                            class="tag"
                                                                                            on:click=move |_| {
                                                                                                set_search_query.set(format!("#{}", t));
                                                                                            }
                                                                                        >
                                                                                            {t.clone()}
                                                                                        </small>
                                                                                    }
                                                                                }).collect::<Vec<_>>()}
                                                                            </div>
                                                                        }.into_any()
                                                                    } else {
                                                                        view! { <></> }.into_any()
                                                                    }
                                                                }}
                                                                {item_hint.map(|hint| {
                                                                    view! { <small class="item-hint">{hint}</small> }
                                                                })}
                                                            </div>
                                                        }
                                                    }).collect::<Vec<_>>()}
                                                </article>
                                            }
                                        }).collect::<Vec<_>>()}
                                    </div>
                                </div>
                            }.into_any()
                        } else {
                            // Search results
                            // Check if query is a tag search (starts with #)
                            let is_tag_search = query.starts_with('#');
                            let tag_query = if is_tag_search {
                                query.trim_start_matches('#')
                            } else {
                                &query
                            };

                            let mut matches: Vec<(usize, usize, f64)> = index.get()
                                .iter()
                                .filter(|entry| entry.sheet_id == sheet.id)
                                .filter_map(|entry| {
                                    let score = if is_tag_search {
                                        // For tag searches, prioritize tag matches
                                        let tag_score = fuzzy_score(tag_query, &entry.tags_text);
                                        if tag_score > 0.0 {
                                            tag_score * 2.0 // Boost tag matches
                                        } else {
                                            // Also search in everything else
                                            fuzzy_score(&query, &entry.text) * 0.5
                                        }
                                    } else {
                                        fuzzy_score(&query, &entry.text)
                                    };

                                    if score > 0.0 {
                                        Some((entry.section_index, entry.item_index, score))
                                    } else {
                                        None
                                    }
                                })
                                .collect();

                            matches.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
                            let match_count = matches.len();
                            matches.truncate(100);

                            view! {
                                <div>
                                    <p class="results-meta">
                                        {format!("{} / {} entries match · Query: \"{}\"", match_count, total_items, query)}
                                    </p>
                                    {if matches.is_empty() {
                                        view! { <p>"No matches."</p> }.into_any()
                                    } else {
                                        matches.into_iter().map(|(sec_idx, item_idx, score)| {
                                            let section = &sheet.sections[sec_idx];
                                            let item = &section.items[item_idx];
                                            let section_title = section.title.clone();
                                            let item_keys = item.keys.clone();
                                            let item_desc = item.desc.clone();
                                            let item_tags = item.tags.clone();
                                            let item_hint = item.hint.clone();
                                            view! {
                                                <article class="search-item">
                                                    <header class="search-path">
                                                        <strong>{section_title}</strong>
                                                        <small class="search-score">{format!("{:.2}", score)}</small>
                                                    </header>
                                                    <div class="cheat-item">
                                                        <div class="key-chips">
                                                            {item_keys.iter().map(|key| {
                                                                let k = key.clone();
                                                                view! { <code class="key-chip">{k}</code> }
                                                            }).collect::<Vec<_>>()}
                                                        </div>
                                                        <div>{item_desc}</div>
                                                        {move || {
                                                            if show_tags.get() && !item_tags.is_empty() {
                                                                view! {
                                                                    <div class="tags">
                                                                        {item_tags.iter().map(|tag| {
                                                                            let t = tag.clone();
                                                                            view! {
                                                                                <small
                                                                                    class="tag"
                                                                                    on:click=move |_| {
                                                                                        set_search_query.set(format!("#{}", t));
                                                                                    }
                                                                                >
                                                                                    {t.clone()}
                                                                                </small>
                                                                            }
                                                                        }).collect::<Vec<_>>()}
                                                                    </div>
                                                                }.into_any()
                                                            } else {
                                                                view! { <></> }.into_any()
                                                            }
                                                        }}
                                                        {item_hint.map(|hint| {
                                                            view! { <small class="item-hint">{hint}</small> }
                                                        })}
                                                    </div>
                                                </article>
                                            }
                                        }).collect::<Vec<_>>().into_any()
                                    }}
                                </div>
                            }.into_any()
                        }
                    })
                }}
            </main>
        </>
    }
}
