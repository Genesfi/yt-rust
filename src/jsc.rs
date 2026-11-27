//! Embedded QuickJS solver for YouTube stream `n` and `signatureCipher` challenges.
//!
//! Uses yt-dlp's EJS solver (`assets/ejs/yt.solver.{lib,core}.min.js`) compiled to
//! cached QuickJS bytecode so bot-checked / signed-in `WEB` InnerTube streams resolve
//! in-process within a few milliseconds without spawning `yt-dlp`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crossbeam_channel::{RecvTimeoutError, Sender};
use rquickjs::{Context, Runtime, qjs};
use serde_json::Value;
use tokio::sync::oneshot;
use url::Url;

const LIB: &str = include_str!("../assets/ejs/yt.solver.lib.min.js");
const CORE: &str = include_str!("../assets/ejs/yt.solver.core.min.js");
const IDLE: Duration = Duration::from_secs(60);
const PLAYER_TTL: Duration = Duration::from_secs(3600);
const JS_LIMIT: Duration = Duration::from_secs(45);

#[derive(Clone, Debug)]
pub struct Player {
    pub id: String,
    pub sts: u32,
}

type Reply<T> = oneshot::Sender<anyhow::Result<T>>;

enum Job {
    Preprocess {
        base: String,
        reply: Reply<String>,
    },
    SolveBatch {
        path: PathBuf,
        ns: Vec<String>,
        sigs: Vec<String>,
        reply: Reply<(HashMap<String, String>, HashMap<String, String>)>,
    },
}

pub struct Solver {
    dir: PathBuf,
    worker: Mutex<Option<Sender<Job>>>,
    player: tokio::sync::Mutex<Option<(Player, Instant)>>,
    failed: Mutex<Option<Instant>>,
}

impl Solver {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            dir: data_dir.join("ejs"),
            worker: Mutex::new(None),
            player: tokio::sync::Mutex::new(None),
            failed: Mutex::new(None),
        }
    }

    pub async fn player(&self, http: &reqwest::Client) -> anyhow::Result<Player> {
        let mut cached = self.player.lock().await;
        if let Some((p, _)) = cached.as_ref().filter(|(_, at)| at.elapsed() < PLAYER_TTL) {
            return Ok(p.clone());
        }
        if self
            .failed
            .lock()
            .unwrap()
            .is_some_and(|at| at.elapsed() < Duration::from_secs(300))
        {
            anyhow::bail!("player solver preparation failed recently");
        }
        let res = self.prepare(http).await;
        match &res {
            Ok(p) => *cached = Some((p.clone(), Instant::now())),
            Err(_) => *self.failed.lock().unwrap() = Some(Instant::now()),
        }
        res
    }

    async fn prepare(&self, http: &reqwest::Client) -> anyhow::Result<Player> {
        let iframe = http
            .get("https://www.youtube.com/iframe_api")
            .timeout(Duration::from_secs(8))
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        let id = player_id(&iframe)
            .ok_or_else(|| anyhow::anyhow!("no player version in iframe_api"))?;
        let script = self.dir.join(format!("{id}.js"));
        let sts = match std::fs::read_to_string(self.dir.join(format!("{id}.sts")))
            .ok()
            .and_then(|s| s.trim().parse().ok())
        {
            Some(sts) if script.exists() => sts,
            _ => {
                let t = Instant::now();
                let url = format!(
                    "https://www.youtube.com/s/player/{id}/player_ias.vflset/en_US/base.js"
                );
                let base = http
                    .get(url)
                    .timeout(Duration::from_secs(15))
                    .send()
                    .await?
                    .error_for_status()?
                    .text()
                    .await?;
                let sts = signature_timestamp(&base)
                    .ok_or_else(|| anyhow::anyhow!("no signatureTimestamp in player {id}"))?;
                let (reply, rx) = oneshot::channel();
                self.send(Job::Preprocess { base, reply })?;
                let pre = rx.await??;
                if let Ok(old) = std::fs::read_dir(&self.dir) {
                    for f in old.flatten() {
                        let _ = std::fs::remove_file(f.path());
                    }
                }
                std::fs::create_dir_all(&self.dir)?;
                std::fs::write(&script, pre)?;
                std::fs::write(self.dir.join(format!("{id}.sts")), sts.to_string())?;
                log::info!("jsc: preprocessed YouTube player {id} in {:?}", t.elapsed());
                sts
            }
        };
        Ok(Player { id, sts })
    }

    pub async fn warm(&self, http: &reqwest::Client) -> anyhow::Result<()> {
        let player = self.player(http).await?;
        let _ = self.solve_batch(&player, Vec::new(), Vec::new()).await?;
        Ok(())
    }

    pub async fn solve_batch(
        &self,
        player: &Player,
        ns: Vec<String>,
        sigs: Vec<String>,
    ) -> anyhow::Result<(HashMap<String, String>, HashMap<String, String>)> {
        let path = self.dir.join(format!("{}.js", player.id));
        let (reply, rx) = oneshot::channel();
        self.send(Job::SolveBatch {
            path: path.clone(),
            ns: ns.clone(),
            sigs: sigs.clone(),
            reply,
        })?;
        match rx.await {
            Ok(res) => res,
            Err(_) => {
                let (reply, rx) = oneshot::channel();
                self.send(Job::SolveBatch {
                    path,
                    ns,
                    sigs,
                    reply,
                })?;
                rx.await?
            }
        }
    }

    /// Resolves `n`, `signatureCipher`, and optional `&pot=` onto every entry in
    /// `streamingData.adaptiveFormats` and `streamingData.formats` in place.
    pub async fn solve_streaming_data(
        &self,
        player: &Player,
        streaming_data: &mut Value,
        pot_stream: Option<&str>,
    ) -> anyhow::Result<()> {
        struct Parsed {
            bucket: &'static str,
            idx: usize,
            raw_url: String,
            n_val: Option<String>,
            sig_val: Option<String>,
            sp: String,
        }

        let mut entries = Vec::new();
        let mut uniq_n = Vec::new();
        let mut seen_n = HashSet::new();
        let mut uniq_sig = Vec::new();
        let mut seen_sig = HashSet::new();

        for bucket in ["adaptiveFormats", "formats"] {
            let Some(arr) = streaming_data[bucket].as_array() else {
                continue;
            };
            for (idx, f) in arr.iter().enumerate() {
                let (raw_url, s_val, sp) = if let Some(u) = f["url"].as_str() {
                    (u.to_owned(), None, String::new())
                } else if let Some(c) = f["signatureCipher"]
                    .as_str()
                    .or_else(|| f["cipher"].as_str())
                {
                    let Ok(parsed_c) = Url::parse(&format!("http://x/?{c}")) else {
                        continue;
                    };
                    let mut url_out = None;
                    let mut sig_out = None;
                    let mut sp_out = None;
                    for (k, v) in parsed_c.query_pairs() {
                        match k.as_ref() {
                            "url" => url_out = Some(v.into_owned()),
                            "s" => sig_out = Some(v.into_owned()),
                            "sp" => sp_out = Some(v.into_owned()),
                            _ => {}
                        }
                    }
                    let Some(u) = url_out else {
                        continue;
                    };
                    (u, sig_out, sp_out.unwrap_or_else(|| "signature".to_owned()))
                } else {
                    continue;
                };

                let Ok(parsed_u) = Url::parse(&raw_url) else {
                    continue;
                };
                let n_val = parsed_u
                    .query_pairs()
                    .find(|(k, _)| k == "n")
                    .map(|(_, v)| v.into_owned());

                if let Some(ref nv) = n_val {
                    if seen_n.insert(nv.clone()) {
                        uniq_n.push(nv.clone());
                    }
                }
                if let Some(ref sv) = s_val {
                    if seen_sig.insert(sv.clone()) {
                        uniq_sig.push(sv.clone());
                    }
                }

                entries.push(Parsed {
                    bucket,
                    idx,
                    raw_url,
                    n_val,
                    sig_val: s_val,
                    sp,
                });
            }
        }

        if entries.is_empty() {
            return Ok(());
        }

        let (n_map, sig_map) = if uniq_n.is_empty() && uniq_sig.is_empty() {
            (HashMap::new(), HashMap::new())
        } else {
            self.solve_batch(player, uniq_n, uniq_sig).await?
        };

        for entry in entries {
            let Ok(mut u) = Url::parse(&entry.raw_url) else {
                continue;
            };
            let mut pairs: Vec<(String, String)> = u.query_pairs().into_owned().collect();
            if let Some(ref nv) = entry.n_val {
                if let Some(solved_n) = n_map.get(nv) {
                    for (k, v) in &mut pairs {
                        if k == "n" {
                            *v = solved_n.clone();
                        }
                    }
                }
            }
            if let Some(ref sv) = entry.sig_val {
                if let Some(solved_s) = sig_map.get(sv) {
                    pairs.retain(|(k, _)| k != &entry.sp);
                    pairs.push((entry.sp.clone(), solved_s.clone()));
                } else {
                    continue;
                }
            }
            if let Some(pot) = pot_stream.filter(|p| !p.is_empty()) {
                pairs.retain(|(k, _)| k != "pot");
                pairs.push(("pot".to_owned(), pot.to_owned()));
            }
            u.query_pairs_mut().clear().extend_pairs(pairs);
            if let Some(obj) = streaming_data[entry.bucket]
                .as_array_mut()
                .and_then(|a| a.get_mut(entry.idx))
                .and_then(Value::as_object_mut)
            {
                obj.insert("url".to_owned(), Value::String(u.into()));
                obj.remove("signatureCipher");
                obj.remove("cipher");
            }
        }

        Ok(())
    }

    fn send(&self, mut job: Job) -> anyhow::Result<()> {
        let mut worker = self.worker.lock().unwrap();
        if let Some(tx) = worker.as_ref() {
            match tx.send(job) {
                Ok(()) => return Ok(()),
                Err(e) => job = e.0,
            }
        }
        let (tx, rx) = crossbeam_channel::unbounded();
        std::thread::Builder::new()
            .name("jsc-solver".into())
            .spawn(move || {
                let mut loaded: Option<(PathBuf, Runtime, Context)> = None;
                loop {
                    match rx.recv_timeout(IDLE) {
                        Ok(Job::Preprocess { base, reply }) => {
                            let _ = reply.send(preprocess(&base));
                        }
                        Ok(Job::SolveBatch {
                            path,
                            ns,
                            sigs,
                            reply,
                        }) => {
                            if loaded.as_ref().is_none_or(|(p, ..)| *p != path) {
                                loaded = None;
                                match load(&path) {
                                    Ok((rt, ctx)) => loaded = Some((path, rt, ctx)),
                                    Err(e) => {
                                        let _ = reply.send(Err(e));
                                        continue;
                                    }
                                }
                            }
                            let (_, rt, ctx) = loaded.as_ref().unwrap();
                            watchdog(rt);
                            let _ = reply.send(solve_batch_ctx(ctx, &ns, &sigs));
                        }
                        Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => return,
                    }
                }
            })?;
        tx.send(job)
            .map_err(|_| anyhow::anyhow!("jsc solver thread failed to start"))?;
        *worker = Some(tx);
        Ok(())
    }
}

fn js_err(ctx: &rquickjs::Ctx, e: rquickjs::Error) -> anyhow::Error {
    if matches!(e, rquickjs::Error::Exception) {
        let ex = ctx.catch();
        let msg = ex
            .as_exception()
            .map(|x| x.to_string())
            .or_else(|| ex.as_string().and_then(|s| s.to_string().ok()));
        anyhow::anyhow!("js: {}", msg.unwrap_or_else(|| "exception".into()))
    } else {
        anyhow::anyhow!("js: {e}")
    }
}

fn watchdog(rt: &Runtime) {
    let deadline = Instant::now() + JS_LIMIT;
    rt.set_interrupt_handler(Some(Box::new(move || Instant::now() >= deadline)));
}

fn preprocess(base: &str) -> anyhow::Result<String> {
    let rt = Runtime::new()?;
    rt.set_max_stack_size(64 << 20);
    watchdog(&rt);
    let ctx = Context::full(&rt)?;
    ctx.with(|ctx| {
        let input = serde_json::json!({
            "type": "player",
            "player": base,
            "output_preprocessed": true,
            "requests": []
        });
        let src = format!(
            "{LIB}\nObject.assign(globalThis, lib);\n{CORE}\nJSON.stringify(jsc({input}))"
        );
        let out: String = ctx.eval(src).map_err(|e| js_err(&ctx, e))?;
        let v: Value = serde_json::from_str(&out)?;
        v["preprocessed_player"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| anyhow::anyhow!("solver returned no player: {out:.200}"))
    })
}

fn load(path: &Path) -> anyhow::Result<(Runtime, Context)> {
    let t = Instant::now();
    let rt = Runtime::new()?;
    rt.set_max_stack_size(64 << 20);
    watchdog(&rt);
    let ctx = Context::full(&rt)?;
    let cached = path.with_extension("qbc");
    let fresh = match std::fs::read(&cached) {
        Ok(bc) => ctx
            .with(|ctx| run_bytecode(&ctx, &bc))
            .inspect_err(|e| log::info!("jsc: cached bytecode unusable ({e:#}); recompiling"))
            .is_err(),
        Err(_) => true,
    };
    if fresh {
        let bc = compile(&std::fs::read_to_string(path)?)?;
        ctx.with(|ctx| run_bytecode(&ctx, &bc))?;
        let _ = std::fs::write(&cached, &bc);
    }
    log::debug!("jsc: loaded solver in {:?}", t.elapsed());
    Ok((rt, ctx))
}

fn compile(pre: &str) -> anyhow::Result<Vec<u8>> {
    let rt = Runtime::new()?;
    rt.set_max_stack_size(64 << 20);
    watchdog(&rt);
    let ctx = Context::full(&rt)?;
    let src = format!(
        "var _r = {{n: null, sig: null}};\n(function (_result) {{\n{pre}\n}})(_r);\n\0"
    );
    ctx.with(|ctx| unsafe {
        let c = ctx.as_raw().as_ptr();
        let flags = (qjs::JS_EVAL_TYPE_GLOBAL | qjs::JS_EVAL_FLAG_COMPILE_ONLY) as i32;
        let f = qjs::JS_Eval(
            c,
            src.as_ptr().cast(),
            (src.len() - 1) as _,
            c"solver.js".as_ptr(),
            flags,
        );
        if qjs::JS_IsException(f) {
            return Err(js_err(&ctx, rquickjs::Error::Exception));
        }
        let mut len = 0;
        let flags = (qjs::JS_WRITE_OBJ_BYTECODE
            | qjs::JS_WRITE_OBJ_STRIP_SOURCE
            | qjs::JS_WRITE_OBJ_STRIP_DEBUG) as i32;
        let buf = qjs::JS_WriteObject(c, &mut len, f, flags);
        qjs::JS_FreeValue(c, f);
        anyhow::ensure!(!buf.is_null(), "js: couldn't write bytecode");
        let out = std::slice::from_raw_parts(buf, len as usize).to_vec();
        qjs::js_free(c, buf.cast());
        Ok(out)
    })
}

fn run_bytecode(ctx: &rquickjs::Ctx, bc: &[u8]) -> anyhow::Result<()> {
    unsafe {
        let c = ctx.as_raw().as_ptr();
        let f = qjs::JS_ReadObject(
            c,
            bc.as_ptr(),
            bc.len() as _,
            qjs::JS_READ_OBJ_BYTECODE as i32,
        );
        if qjs::JS_IsException(f) {
            return Err(js_err(ctx, rquickjs::Error::Exception));
        }
        let r = qjs::JS_EvalFunction(c, f);
        if qjs::JS_IsException(r) {
            return Err(js_err(ctx, rquickjs::Error::Exception));
        }
        qjs::JS_FreeValue(c, r);
    }
    Ok(())
}

fn solve_batch_ctx(
    ctx: &Context,
    ns: &[String],
    sigs: &[String],
) -> anyhow::Result<(HashMap<String, String>, HashMap<String, String>)> {
    ctx.with(|ctx| {
        let mut n_out = HashMap::with_capacity(ns.len());
        let mut sig_out = HashMap::with_capacity(sigs.len());
        for input in ns {
            let src = format!("_r.n({})", serde_json::to_string(input)?);
            let val: String = ctx.eval(src).map_err(|e| js_err(&ctx, e))?;
            n_out.insert(input.clone(), val);
        }
        for input in sigs {
            let src = format!("_r.sig({})", serde_json::to_string(input)?);
            let val: String = ctx.eval(src).map_err(|e| js_err(&ctx, e))?;
            sig_out.insert(input.clone(), val);
        }
        Ok((n_out, sig_out))
    })
}

pub fn player_id(iframe: &str) -> Option<String> {
    iframe.match_indices("player").find_map(|(i, _)| {
        let rest = iframe[i + 6..].trim_start_matches(['\\', '/']);
        let id = rest.get(..8)?;
        (id.bytes().all(|b| b.is_ascii_hexdigit()) && rest[8..].starts_with(['\\', '/']))
            .then(|| id.to_owned())
    })
}

pub fn signature_timestamp(base: &str) -> Option<u32> {
    ["signatureTimestamp", "sts"].iter().find_map(|key| {
        base.match_indices(key).find_map(|(i, _)| {
            let rest = base[i + key.len()..].trim_start().strip_prefix(':')?.trim_start();
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            (digits.len() == 5).then(|| digits.parse().ok()).flatten()
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_player_id_and_sts() {
        assert_eq!(
            player_id(
                r"var a='https:\/\/www.youtube.com\/s\/player\/5203c085\/www-widgetapi.vflset'"
            )
            .as_deref(),
            Some("5203c085")
        );
        assert_eq!(
            signature_timestamp("x={a:1,signatureTimestamp:20732,b:2}"),
            Some(20732)
        );
        assert_eq!(
            signature_timestamp("sts:123,sts:20733"),
            Some(20733)
        );
    }

    #[test]
    fn appends_pot_to_plain_streaming_urls() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let solver = Solver::new(Path::new("."));
        let player = Player {
            id: "00000000".to_owned(),
            sts: 20732,
        };
        let mut sd = serde_json::json!({
            "adaptiveFormats": [
                { "itag": 137, "url": "https://example.com/videoplayback?itag=137&pot=old" },
                { "itag": 140, "url": "https://example.com/videoplayback?itag=140" }
            ]
        });
        rt.block_on(solver.solve_streaming_data(&player, &mut sd, Some("new_pot_tok")))
            .unwrap();
        let v_url = sd["adaptiveFormats"][0]["url"].as_str().unwrap();
        let a_url = sd["adaptiveFormats"][1]["url"].as_str().unwrap();
        assert!(v_url.contains("pot=new_pot_tok") && !v_url.contains("pot=old"));
        assert!(a_url.contains("pot=new_pot_tok"));
    }
}
