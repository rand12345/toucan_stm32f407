use std::env;
use std::fs::File;
use std::io::Write;
use std::path::Path;

fn main() {
    let dest_path = Path::new("./src/bin/tasks/ntp/timezone.rs");
    let mut f = File::create(dest_path).unwrap();

    let timezone = env::var("TIMEZONE").unwrap_or_else(|_| "Europe/London".to_string());

    // Mapping the environment variable to the actual enum variant
    let tz_code = match timezone.as_str() {
        "UTC" => "chrono_tz::UTC",
        "GMT" => "chrono_tz::GMT",
        "America/New_York" => "chrono_tz::America::New_York",
        "America/Los_Angeles" => "chrono_tz::America::Los_Angeles",
        "America/Chicago" => "chrono_tz::America::Chicago",
        "America/Denver" => "chrono_tz::America::Denver",
        "Europe/London" => "chrono_tz::Europe::London",
        "Europe/Berlin" => "chrono_tz::Europe::Berlin",
        "Europe/Paris" => "chrono_tz::Europe::Paris",
        "Europe/Moscow" => "chrono_tz::Europe::Moscow",
        "Europe/Rome" => "chrono_tz::Europe::Rome",
        _ => panic!("Unsupported timezone: {}", timezone),
    };

    f.write_all(format!("const TZ: chrono_tz::Tz = {};\n", tz_code).as_bytes())
        .unwrap();
}
