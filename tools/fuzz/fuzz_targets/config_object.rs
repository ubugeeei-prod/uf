#![no_main]

use libfuzzer_sys::fuzz_target;
use uf_flow::FlowParser;

fuzz_target!(|data: &[u8]| {
    let Ok(source) = std::str::from_utf8(data) else {
        return;
    };

    let _ = uf_config::extract_config_object(source);
    let _ = FlowParser.validate_source(source);
});
