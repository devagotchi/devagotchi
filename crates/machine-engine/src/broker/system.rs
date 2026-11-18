//! The machine beside its files: settings (see `settings`), processes, and the web as the
//! machine sees it (see `web`). `list_processes`
//! and `kill_process` reach every process the broker's OS account can see, so only a broker
//! that runs what its account can (exec policy `account`) offers them; a `granted` broker's
//! own jobs are in `operation_list` and stop with `operation_cancel`.
//!
//! `kill_process` never stops Devagotchi's own processes (the broker, its link, an adapter)
//! or what the broker runs under, which would cut the connection asking, and sends a job's root process to
//! `operation_cancel`, which stops its whole group and settles its record.

use super::*;

mod web;
use sysinfo::{
    Pid, ProcessRefreshKind, ProcessStatus, ProcessesToUpdate, Signal, System, UpdateKind, Users,
};

/// Longest a command line is shown.
const COMMAND_CHARS: usize = 400;
/// Why a granted broker lists and stops no process.
const PROCESSES: &str = "so it does not reach the machine's other processes; its own jobs are in operation_list and stop with operation_cancel";
/// How long `kill_process` watches for the process to go.
const EXIT_WATCH: Duration = Duration::from_secs(3);

impl Broker {
    pub(super) async fn on_system(&self, request: RequestBody) -> BridgeResult<ResponseBody> {
        let (what, done) = match request {
            RequestBody::ConfigGet(_) => (
                "get_config",
                self.config_view(None).map(ResponseBody::Config),
            ),
            RequestBody::BrokerCapabilities(_) => (
                "broker_capabilities",
                self.capabilities().map(ResponseBody::Capabilities),
            ),
            RequestBody::ConfigSet(input) => (
                "set_config_value",
                self.set_config(input).map(ResponseBody::Config),
            ),
            RequestBody::ProcessList(input) => (
                "list_processes",
                self.processes(input).await.map(ResponseBody::Processes),
            ),
            RequestBody::ProcessKill(input) => (
                "kill_process",
                self.kill(input).await.map(ResponseBody::Killed),
            ),
            RequestBody::UrlRead(input) => (
                "read_url",
                match self.account_only("so it reaches no web address") {
                    Ok(()) => web::read(input).await.map(ResponseBody::Url),
                    Err(error) => Err(error),
                },
            ),
            _ => (
                "system request",
                Err(BridgeError::new("internal_error", "not a system request")),
            ),
        };
        if let Err(error) = &done {
            trace::refused(what, error);
        }
        done
    }

    /// Refused on a broker that runs only granted programs, `so` what it does not do.
    fn account_only(&self, so: &str) -> BridgeResult<()> {
        if self.shared.config.exec_policy == ExecPolicy::Granted {
            return Err(BridgeError::new(
                "not_permitted",
                format!("This broker runs only granted programs, {so}"),
            ));
        }
        Ok(())
    }

    /// Each running job's root process, by PID.
    fn job_roots(&self) -> HashMap<u32, String> {
        let state = self.shared.lock();
        state
            .operations
            .iter()
            .filter(|(_, operation)| operation.active)
            .filter_map(|(id, operation)| operation.pid.map(|pid| (pid, id.clone())))
            .collect()
    }

    async fn processes(&self, input: ProcessListInput) -> BridgeResult<ProcessPage> {
        self.account_only(PROCESSES)?;
        if !(1..=MAX_PROCESS_RESULTS).contains(&input.max_results) {
            return Err(invalid(format!(
                "max_results must be 1..={MAX_PROCESS_RESULTS}"
            )));
        }
        let roots = self.job_roots();
        blocking(move || {
            let table = Table::read(true)?;
            Ok(table.page(&roots, input.filter.as_deref(), input.max_results as usize))
        })
        .await
    }

    async fn kill(&self, input: ProcessKillInput) -> BridgeResult<ProcessKilled> {
        self.account_only(PROCESSES)?;
        let roots = self.job_roots();
        if let Some(id) = roots.get(&input.pid) {
            return Err(BridgeError::new(
                "operation_process",
                format!(
                    "PID {} is job {id}'s own process; operation_cancel stops it with everything it started and settles the job",
                    input.pid
                ),
            ));
        }
        blocking(move || {
            let table = Table::read(false)?;
            let pid = Pid::from_u32(input.pid);
            let Some(process) = table.system.process(pid) else {
                return Err(BridgeError::new("process_not_found", format!("No process has PID {} here", input.pid)));
            };
            let name = process.name().to_string_lossy().into_owned();
            if table.broker.contains(&input.pid) {
                return Err(BridgeError::new(
                    "broker_process",
                    format!("PID {} ({name}) is Devagotchi's own (the broker, its link, an adapter, or what the broker runs under); stopping it would cut this connection. Stop Devagotchi on the machine itself", input.pid),
                ));
            }
            let (signal, sent) = if input.force {
                ("kill", process.kill_with(Signal::Kill).unwrap_or_else(|| process.kill()))
            } else {
                match process.kill_with(Signal::Term) {
                    Some(sent) => ("terminate", sent),
                    // Windows has no polite stop for another process: it is ended at once.
                    None => ("kill", process.kill()),
                }
            };
            if !sent {
                return Err(BridgeError::new(
                    "kill_failed",
                    format!("The OS refused to signal PID {} ({name}); it may belong to another account", input.pid),
                ));
            }
            trace::line("killed", &format!("PID {} ({name}) with {signal}", input.pid));
            Ok(ProcessKilled { pid: input.pid, name, signal: signal.into(), exited: gone_within(pid, EXIT_WATCH) })
        })
        .await
    }
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> BridgeResult<T> + Send + 'static,
) -> BridgeResult<T> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|error| BridgeError::new("internal_error", error.to_string()))?
}

/// Whether `pid` is gone (or only a zombie waiting for its parent) within `limit`.
fn gone_within(pid: Pid, limit: Duration) -> bool {
    let Ok(mut system) = System::new() else {
        return false;
    };
    let started = std::time::Instant::now();
    loop {
        system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[pid]),
            true,
            ProcessRefreshKind::nothing(),
        );
        match system.process(pid) {
            None => return true,
            Some(process) if process.status() == ProcessStatus::Zombie => return true,
            Some(_) if started.elapsed() >= limit => return false,
            Some(_) => std::thread::sleep(Duration::from_millis(100)),
        }
    }
}

/// The machine's processes, and which of them the broker runs under.
struct Table {
    system: System,
    /// This broker's PID and its ancestors', and every process of the broker's own program
    /// (the link, an adapter): the processes `kill_process` never stops.
    broker: HashSet<u32>,
}

impl Table {
    /// Read the table; with `cpu`, twice over a short sample, so CPU use means something.
    fn read(cpu: bool) -> BridgeResult<Self> {
        let failed = |error: sysinfo::Error| {
            BridgeError::new(
                "internal_error",
                format!("The process table could not be read: {error}"),
            )
        };
        let mut system = System::new().map_err(failed)?;
        let kind = ProcessRefreshKind::nothing()
            .with_memory()
            .with_cmd(UpdateKind::OnlyIfNotSet)
            .with_user(UpdateKind::OnlyIfNotSet)
            .with_exe(UpdateKind::OnlyIfNotSet)
            .without_tasks();
        let kind = if cpu { kind.with_cpu() } else { kind };
        system.refresh_processes_specifics(ProcessesToUpdate::All, true, kind);
        if cpu {
            std::thread::sleep(
                sysinfo::MINIMUM_CPU_UPDATE_INTERVAL.max(Duration::from_millis(200)),
            );
            system.refresh_processes_specifics(ProcessesToUpdate::All, true, kind);
        }
        let mut broker = HashSet::new();
        let mut next = Some(Pid::from_u32(std::process::id()));
        while let Some(pid) = next {
            if !broker.insert(pid.as_u32()) {
                break;
            }
            next = system.process(pid).and_then(|process| process.parent());
        }
        if let Ok(own) = std::env::current_exe().and_then(dunce::canonicalize) {
            let ours = |exe: &Path| dunce::canonicalize(exe).is_ok_and(|exe| exe == own);
            broker.extend(
                system
                    .processes()
                    .iter()
                    .filter(|(_, process)| process.exe().is_some_and(ours))
                    .map(|(pid, _)| pid.as_u32()),
            );
        }
        Ok(Self { system, broker })
    }

    /// The job a process belongs to: its own, or its nearest ancestor's.
    fn job(&self, pid: Pid, roots: &HashMap<u32, String>) -> Option<String> {
        let mut next = Some(pid);
        for _ in 0..64 {
            let pid = next?;
            if let Some(id) = roots.get(&pid.as_u32()) {
                return Some(id.clone());
            }
            next = self
                .system
                .process(pid)
                .and_then(|process| process.parent());
        }
        None
    }

    fn page(&self, roots: &HashMap<u32, String>, filter: Option<&str>, most: usize) -> ProcessPage {
        let users = Users::new_with_refreshed_list().ok();
        let filter = filter
            .map(str::to_lowercase)
            .filter(|filter| !filter.is_empty());
        let mut found: Vec<ProcessInfo> = self
            .system
            .processes()
            .iter()
            .map(|(pid, process)| {
                let name = process.name().to_string_lossy().into_owned();
                let mut command = process
                    .cmd()
                    .iter()
                    .map(|part| part.to_string_lossy())
                    .collect::<Vec<_>>()
                    .join(" ");
                if command.is_empty() {
                    command = process
                        .exe()
                        .map_or_else(|| name.clone(), |exe| exe.display().to_string());
                }
                ProcessInfo {
                    pid: pid.as_u32(),
                    parent_pid: process.parent().map(Pid::as_u32),
                    command: trace::shorten(command, COMMAND_CHARS),
                    cpu_percent: (f64::from(process.cpu_usage()) * 10.0).round() / 10.0,
                    memory_bytes: process.memory(),
                    started_at_ms: Some(process.start_time() * 1000).filter(|ms| *ms > 0),
                    user: process
                        .user_id()
                        .and_then(|id| users.as_ref()?.get_user_by_id(id))
                        .map(|user| user.name().to_string()),
                    operation_id: self.job(*pid, roots),
                    broker: self.broker.contains(&pid.as_u32()),
                    name,
                }
            })
            .filter(|info| {
                filter.as_ref().is_none_or(|filter| {
                    info.name.to_lowercase().contains(filter)
                        || info.command.to_lowercase().contains(filter)
                })
            })
            .collect();
        let matched = found.len() as u64;
        found.sort_by(|a, b| {
            b.cpu_percent
                .total_cmp(&a.cpu_percent)
                .then(b.memory_bytes.cmp(&a.memory_bytes))
                .then(a.pid.cmp(&b.pid))
        });
        found.truncate(most);
        ProcessPage {
            processes: found,
            matched,
            total: self.system.processes().len() as u64,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn the_table_knows_the_broker_and_its_jobs() {
        let mut child = std::process::Command::new("sleep")
            .arg("5")
            .spawn()
            .unwrap();
        let mine = std::process::id();
        let roots = HashMap::from([(child.id(), "job-1".to_string())]);
        let table = Table::read(false).unwrap();
        assert!(table.broker.contains(&mine));
        let page = table.page(&roots, Some("sleep"), 1000);
        let found = page
            .processes
            .iter()
            .find(|info| info.pid == child.id())
            .expect("the child is listed");
        assert_eq!(found.operation_id.as_deref(), Some("job-1"));
        assert_eq!(found.parent_pid, Some(mine));
        assert!(
            page.processes
                .iter()
                .all(|info| info.name.contains("sleep") || info.command.contains("sleep"))
        );
        child.kill().unwrap();
        child.wait().unwrap();
    }
}
