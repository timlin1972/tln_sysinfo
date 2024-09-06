use anstream::println;
use chrono::{DateTime, Local};
use owo_colors::OwoColorize as _;
use readable::{byte::*, num::*, up::*};
use serde::{Deserialize, Serialize};
use sysinfo::{Components, Disks, System};

use common::{cfg, plugin, utils};

const MODULE: &str = "sysinfo";

#[derive(Serialize, Deserialize, Debug)]
struct Report {
    topic: String,
    payload: String,
}

pub struct Plugin {
    start_ts: u64,
    sys: System,
    tx: crossbeam_channel::Sender<String>,
}

impl Plugin {
    pub fn new(tx: &crossbeam_channel::Sender<String>) -> Plugin {
        println!("[{}] Loading...", MODULE.blue());

        let start_ts = utils::get_ts();
        let sys = System::new_all();

        Plugin {
            start_ts,
            sys,
            tx: tx.clone(),
        }
    }

    fn get_sw_uptime(&self) -> u64 {
        utils::get_ts() - self.start_ts
    }
}

fn get_temperature() -> String {
    let components = Components::new_with_refreshed_list();
    for component in &components {
        if component.label().to_ascii_lowercase().contains("cpu") {
            return component.temperature().to_string();
        }
    }
    "0".to_owned()
}

impl plugin::Plugin for Plugin {
    fn name(&self) -> &str {
        MODULE
    }

    fn status(&mut self) -> String {
        println!("[{}]", MODULE.blue());

        let mut status = String::new();

        // Software Info
        status += "Software Info:\n";
        status += &format!("\tUptime: {}\n", Uptime::from(self.get_sw_uptime()));

        // IP Info
        let response = reqwest::blocking::get("https://api.ipify.org?format=text").unwrap();
        let ip = response.text().unwrap();

        status += "IP Info:\n";
        status += &format!("\tWAN IP: {ip}\n");

        // System Info
        let sys = &mut self.sys;
        sys.refresh_all();

        let datetime_local: DateTime<Local> =
            DateTime::from_timestamp(System::boot_time() as i64, 0)
                .unwrap()
                .with_timezone(&Local);

        status += "System Info:\n";
        status += &format!("\tOS: {}\n", System::name().unwrap());
        status += &format!("\tKernel Version: {}\n", System::kernel_version().unwrap());
        status += &format!("\tOS Version: {}\n", System::long_os_version().unwrap());
        status += &format!("\tHost Name: {}\n", System::host_name().unwrap());
        status += &format!("\tCPU Architecture: {}\n", System::cpu_arch().unwrap());
        status += &format!("\tNB CPUs: {}\n", sys.cpus().len());
        status += &format!("\tUptime: {} seconds\n", Uptime::from(System::uptime()));
        status += &format!(
            "\tBooted: {}\n",
            datetime_local.format("%Y-%m-%d %H:%M:%S %:z")
        );

        // Temperature Info
        status += "Temperature Info:\n";
        status += &format!("\tTemperature: {}°C\n", get_temperature());

        // CPU Usage
        status += "CPU Usage:\n";
        let load_avg = System::load_average();
        status += &format!(
            "\tone minute: {}%, five minutes: {}%, fifteen minutes: {}%\n",
            load_avg.one, load_avg.five, load_avg.fifteen,
        );

        // Memory Info
        status += "Memory Info:\n";
        let available_memory = sys.available_memory();
        let total_memory = sys.total_memory();
        status += &format!(
            "\t{}/{} ({})\n",
            Byte::from(available_memory),
            Byte::from(total_memory),
            Percent::from(available_memory * 100 / total_memory)
        );

        // Disk Info
        status += "Disk Info:\n";
        let disks = Disks::new_with_refreshed_list();
        for disk in disks.list() {
            let available_space: f64 = disk.available_space() as f64;
            let total_space: f64 = disk.total_space() as f64;
            status += &format!(
                "\t{}: {}, {}, {}/{} ({})\n",
                disk.name().to_str().unwrap(),
                disk.kind(),
                disk.file_system().to_str().unwrap(),
                Byte::from(available_space),
                Byte::from(total_space),
                Percent::from(available_space * 100.0 / total_space)
            );
        }

        println!("{status}");

        status
    }

    fn action(&mut self, action: &str, data: &str, _data2: &str) -> String {
        fn send_report(tx: &crossbeam_channel::Sender<String>, topic: &str, payload: String) {
            let report = Report {
                topic: format!("tln/{}/{topic}", cfg::get_name()),
                payload,
            };
            let json_string = serde_json::to_string(&report).unwrap();
            tx.send(format!("send plugin mqtt report '{json_string}'"))
                .unwrap();
        }

        if action == "report" {
            match data {
                "myself" => {
                    send_report(&self.tx, "uptime", System::uptime().to_string());
                    send_report(&self.tx, "sw_uptime", self.get_sw_uptime().to_string());
                    send_report(&self.tx, "hostname", System::host_name().unwrap());
                    send_report(&self.tx, "os", System::name().unwrap());
                    send_report(&self.tx, "temperature", get_temperature());
                }
                "status" => {
                    let status = self.status();
                    send_report(&self.tx, "status", status);
                }
                _ => (),
            }
        }

        "send".to_owned()
    }
}

#[no_mangle]
pub extern "C" fn create_plugin(
    tx: &crossbeam_channel::Sender<String>,
) -> *mut plugin::PluginWrapper {
    let plugin = Box::new(Plugin::new(tx));
    Box::into_raw(Box::new(plugin::PluginWrapper::new(plugin)))
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn unload_plugin(wrapper: *mut plugin::PluginWrapper) {
    if !wrapper.is_null() {
        unsafe {
            let _ = Box::from_raw(wrapper);
        }
    }
}
