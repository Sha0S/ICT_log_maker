#![allow(non_snake_case)]
#![warn(dead_code)]

use rand::prelude::*;
use std::path::PathBuf;

use eframe::egui;
use egui::*;

use chrono::{prelude::*, Duration};

fn main() -> Result<(), eframe::Error> {
    env_logger::init(); // Log to stderr (if you run with `RUST_LOG=debug`).

    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default(),
        ..Default::default()
    };

    eframe::run_native(
        "ICT Logfile Maker".as_str(),
        options,
        Box::new(|_cc| Box::<MyApp>::default()),
    )
}

// Test type + limits (min, nom, max)
enum TType {
    Pin,
    Capacitor(f32, f32, f32),
    Resistor(f32, f32, f32),
}

struct Test {
    name: String,
    ttype: TType,
}

impl Test {
    fn get_measurement(&self, is_ok: bool) -> f32 {
        match self.ttype {
            TType::Pin => 0.0,
            TType::Capacitor(min, _, max) | TType::Resistor(min, _, max) => {
                if is_ok {
                    rand::thread_rng().gen_range(min..max)
                } else {
                    // ToDo: could add a failure mode, when meas > max
                    rand::thread_rng().gen_range(0.0..min)
                }
            }
        }
    }
}

// Create dummy tests.
// 1x pin test
// 10x capacitor test (limits are +- 10-30%)
// 10x resistor test (limits are +- 1-5%)
fn populate_tests() -> Vec<Test> {
    use TType::*;
    let mut ret: Vec<Test> = vec![Test {
        name: "pins".to_string(),
        ttype: Pin,
    }];

    let mut rng = rand::thread_rng();
    for i in 1..=10 {
        let nominal: f32 = rng.gen_range(1E-12..1E-6);
        let min = nominal * rng.gen_range(0.7..0.9);
        let max = nominal * rng.gen_range(1.1..1.3);
        ret.push(Test {
            name: format!("c{i:02.0}"),
            ttype: Capacitor(min, nominal, max),
        })
    }

    let mut rng = rand::thread_rng();
    for i in 1..=10 {
        let nominal: f32 = rng.gen_range(1E0..1E6);
        let min = nominal * rng.gen_range(0.95..0.99);
        let max = nominal * rng.gen_range(1.01..1.05);
        ret.push(Test {
            name: format!("r{i:02.0}"),
            ttype: Resistor(min, nominal, max),
        })
    }

    ret
}

struct TResult {
    ok: bool,
    measured: f32,
}

impl TResult {
    fn to_short(&self) -> &str {
        if self.ok {
            return "0";
        }

        "1"
    }

    fn to_str(&self) -> &str {
        if self.ok {
            return "00";
        }

        "01"
    }
}

struct Board {
    DMC: String,
    index: u8,
    results: Vec<TResult>,
}

impl Board {
    fn get_result(&self) -> &str {
        for res in &self.results {
            if !res.ok {
                return "01";
            }
        }

        "00"
    }
}

#[derive(Default)]
struct MultiBoard {
    DMC: String,
    boards: Vec<Board>,
}

struct MyApp {
    output_dir: PathBuf,

    enabled: bool,

    test_yield: u8, //0-100
    panels: u8,
    testing_time: i64,

    start_time: String,
    last_export: DateTime<Local>,

    last_id: u16,
    tests: Vec<Test>,
    multiboard: MultiBoard,
}

impl MyApp {
    fn its_time(&self) -> bool {
        self.enabled && (Local::now() - self.last_export > Duration::seconds(self.testing_time))
    }

    fn should_pass(&self) -> bool {
        rand::thread_rng().gen_range(0..100) < self.test_yield
    }

    fn generate_results(&self) -> Vec<TResult> {
        let mut ret: Vec<TResult> = Vec::new();

        // ToDo: only produce analog tests if pins test is OK.
        for test in &self.tests {
            let is_ok = self.should_pass();
            ret.push(TResult {
                ok: is_ok,
                measured: test.get_measurement(is_ok),
            })
        }

        ret
    }

    fn generate_DMC(&self, index: u8) -> String {
        let date: NaiveDate = Local::now().date_naive();
        let YY = date.year(); // will return 2024, but we only need the second half? Can use the first half as line ID.
        let DoY = date.ordinal();

        format!(
            "L{YY:04.0}{DoY:03.0}{:05.0}TB0001010111",
            self.last_id + index as u16
        )
    }

    fn generate_multiboard(&mut self) {
        self.multiboard.boards.clear();

        self.multiboard.DMC = self.generate_DMC(0);
        for i in 0..self.panels {
            self.multiboard.boards.push(Board {
                DMC: self.generate_DMC(i),
                index: i + 1,
                results: self.generate_results(),
            })
        }
    }

    fn update_fields(&mut self) {
        self.last_export = Local::now();
        self.last_id += self.panels as u16;
    }

    fn generate_filename(&self, time_now: DateTime<Local>, index: u8) -> String {
        format!("{index}-{}I3070CE0101BZ01", time_now.format("%y%m%d%H%M%S"))
    }

    fn generate_log(&self, board: &Board, start: &String) -> String {
        let mut lines: Vec<String> = Vec::new();

        lines.push(format!(
            "{{@BATCH|DUMMY||0101|1||btest|{}||i30704CE0101BZ01|DUMMY|RevA|DUMMY||D",
            self.start_time
        ));
        lines.push(format!(
            "{{@BTEST|{}|{}|{}|000000|0|all||n|n|{}||{:02.0}|{}",
            board.DMC,
            board.get_result(),
            start,
            Local::now().format("%y%m%d%H%M%S"),
            board.index,
            self.multiboard.DMC
        ));

        for (test, result) in self.tests.iter().zip(board.results.iter()) {
            match test.ttype {
                TType::Pin => {
                    lines.push(format!(
                        "{{@PF|{}%pins|{}|0",
                        board.index,
                        result.to_short()
                    ));
                    lines.push("}".to_string());
                }
                TType::Capacitor(min, nom, max) => {
                    lines.push(format!(
                        "{{@BLOCK|{}%{}|{}",
                        board.index,
                        test.name,
                        result.to_str()
                    ));
                    lines.push(format!(
                        "{{@A-CAP|{}|{:+E}{{@LIM3|{:+E}|{:+E}|{:+E}}}}}",
                        result.to_short(),
                        result.measured,
                        nom,
                        max,
                        min
                    ));
                    lines.push("}".to_string());
                }
                TType::Resistor(min, nom, max) => {
                    lines.push(format!(
                        "{{@BLOCK|{}%{}|{}",
                        board.index,
                        test.name,
                        result.to_str()
                    ));
                    lines.push(format!(
                        "{{@A-RES|{}|{:+E}{{@LIM3|{:+E}|{:+E}|{:+E}}}}}",
                        result.to_short(),
                        result.measured,
                        nom,
                        max,
                        min
                    ));
                    lines.push("}".to_string());
                }
            }
        }

        lines.push("}}".to_string());
        lines.join("\n")
    }

    fn save_results(&self) -> std::io::Result<()> {
        let now = Local::now();
        let start_t = format!("{}", self.last_export.format("%y%m%d%H%M%S"));

        for board in &self.multiboard.boards {
            let path = self
                .output_dir
                .join(self.generate_filename(now, board.index));
            println!("New path: {:?}", path);
            std::fs::write(path, self.generate_log(board, &start_t))?;
        }

        Ok(())
    }
}

impl Default for MyApp {
    fn default() -> Self {
        Self {
            output_dir: PathBuf::from("D:\\Rust\\_Logs\\Dummy"),
            enabled: false,
            test_yield: 99,
            panels: 20,
            testing_time: 30,
            last_export: Local::now(),
            start_time: format!("{}", Local::now().format("%y%m%d%H%M%S")),
            last_id: 1,
            tests: populate_tests(),
            multiboard: MultiBoard::default(),
        }
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(std::time::Duration::from_secs(1));

        if self.its_time() {
            self.generate_multiboard();
            self.save_results().expect("ERR: Saving reults failed!");
            self.update_fields()
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.checkbox(&mut self.enabled, "Enable");
            ui.monospace(format!("Last ID:{}", self.last_id));
            ui.add(egui::Slider::new(&mut self.panels, 1..=20).text("Panels on MB"));
            ui.add(egui::Slider::new(&mut self.testing_time, 5..=60).text("Test time"));
        });
    }
}
