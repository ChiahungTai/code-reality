use code_reality_lsp_bridge::{framing::{read_message,write_message},session::{BackendCommand,LangSpec,LspSession},server::{check_file_impl,status_line}};
use serde_json::{json,Value};
use std::{io::BufReader,path::PathBuf,sync::{Arc,mpsc},time::{Duration,Instant}};
fn root()->PathBuf { std::env::current_dir().unwrap().join("audit/20260927/evidence/lsp") }
fn session(mode:&str)->Arc<LspSession> { Arc::new(LspSession::new(BackendCommand{program:std::env::current_exe().unwrap().to_string_lossy().into(),args:vec!["backend".into(),mode.into()]},root(),50,LangSpec::python())) }
fn kill(pid:u32) { assert!(std::process::Command::new("kill").args(["-9",&pid.to_string()]).status().unwrap().success()); }
fn wait_dead(s:&LspSession) { for _ in 0..200 { if s.is_dead(){return} std::thread::sleep(Duration::from_millis(10)); } panic!("reader did not see death") }
fn backend(mode:&str) {
 let mut input=BufReader::new(std::io::stdin()); let mut output=std::io::stdout();
 while let Some(m)=read_message(&mut input).unwrap() {
  let method=m["method"].as_str().unwrap_or("");
  if method=="initialize" {
   let marker=root().join("init-attempt");
   if mode=="reject-once" && !marker.exists() { std::fs::write(marker,"attempted").unwrap(); write_message(&mut output,&json!({"jsonrpc":"2.0","id":m["id"],"error":{"code":-32603,"message":"transient initialization failure"}})).unwrap(); continue; }
   write_message(&mut output,&json!({"jsonrpc":"2.0","id":m["id"],"result":{"capabilities":{},"serverInfo":{"name":"audit-fake","version":"1"}}})).unwrap();
  } else if method=="stop-reading" { write_message(&mut output,&json!({"jsonrpc":"2.0","id":m["id"],"result":null})).unwrap(); std::thread::sleep(Duration::from_secs(120)); return;
  } else if method=="textDocument/didOpen" { let doc=&m["params"]["textDocument"]; write_message(&mut output,&json!({"jsonrpc":"2.0","method":"textDocument/publishDiagnostics","params":{"uri":doc["uri"],"version":doc["version"],"diagnostics":[]}})).unwrap();
  } else if method=="exit" {return}
  else if m.get("id").is_some() {write_message(&mut output,&json!({"jsonrpc":"2.0","id":m["id"],"result":null})).unwrap();}
 }
}
fn main() {
 let args:Vec<String>=std::env::args().collect(); if args.get(1).map(String::as_str)==Some("backend") {backend(&args[2]);return}
 let mode=args.get(1).expect("mode");
 if mode=="retry" {
  let marker=root().join("init-attempt"); if marker.exists(){std::fs::remove_file(&marker).unwrap();}
  let s=session("reject-once"); let first=s.request("probe",Value::Null); println!("first={first:?}"); assert!(first.is_err()); wait_dead(&s);
  let second=s.request("probe",Value::Null); println!("second={second:?}; dead={}; pid={:?}",s.is_dead(),s.backend_pid()); assert!(second.as_ref().unwrap_err().contains("died"));
  let fresh=session("reject-once"); let control=fresh.request("probe",Value::Null); println!("fresh_session_control={control:?}"); assert!(control.is_ok()); fresh.shutdown().unwrap();
 } else if mode=="dead-cache" {
  let file=root().join("cache-fixture.py");std::fs::write(&file,"x = 1\n").unwrap();let file=file.to_str().unwrap();let s=session("normal");
  let initial=check_file_impl(&s,file).unwrap();println!("initial={initial:?}");kill(s.backend_pid().unwrap());wait_dead(&s);
  println!("status={}",status_line("py",&s));let t=Instant::now();let after=check_file_impl(&s,file);println!("after_death={after:?}; elapsed_ms={}",t.elapsed().as_millis());assert_eq!(after.unwrap(),initial);s.shutdown().unwrap();
 } else if mode=="blocked-write" {
  let file=root().join("large-fixture.py");std::fs::write(&file,format!("# {}\n","x".repeat(4*1024*1024))).unwrap();let file=file.to_string_lossy().into_owned();let s=session("normal");
  s.request("stop-reading",Value::Null).unwrap();let pid=s.backend_pid().unwrap();let (tx,rx)=mpsc::channel();let sc=Arc::clone(&s);let start=Instant::now();
  let worker=std::thread::spawn(move|| {tx.send(check_file_impl(&sc,&file)).unwrap();});
  std::thread::sleep(Duration::from_millis(250));let (stx,srx)=mpsc::channel();let ss=Arc::clone(&s);let shutdown=std::thread::spawn(move||{stx.send(ss.shutdown()).unwrap();});
  let r=rx.recv_timeout(Duration::from_secs(35));println!("check_after_ms={}; result={r:?}; configured_check_deadline_ms={}",start.elapsed().as_millis(),s.lang.slow_timeout_ms);assert!(matches!(r,Err(mpsc::RecvTimeoutError::Timeout)));
  let sr=srx.try_recv();println!("shutdown_after_ms={}; result={sr:?}",start.elapsed().as_millis());assert!(matches!(sr,Err(mpsc::TryRecvError::Empty)));
  kill(pid);println!("external_kill_releases_check={:?}",rx.recv_timeout(Duration::from_secs(5)).unwrap());println!("external_kill_releases_shutdown={:?}",srx.recv_timeout(Duration::from_secs(5)).unwrap());worker.join().unwrap();shutdown.join().unwrap();
 } else {panic!("unknown mode")}
}
