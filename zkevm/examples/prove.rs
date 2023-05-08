use chrono::Utc;
use halo2_proofs::{plonk::keygen_vk, SerdeFormat};
use rand::SeedableRng;
use rand_xorshift::XorShiftRng;
use zkevm::{
    circuit::{SuperCircuit, TargetCircuit, DEGREE},
    io::serialize_vk,
    prover::Prover,
    utils::{load_or_create_params, load_params},
};

use git_version::git_version;
use glob::glob;
use std::sync::Once;
use types::eth::BlockTrace;
use zkevm::utils::get_block_trace_from_file;
use zkevm::utils::read_env_var;

pub const GIT_VERSION: &str = git_version!();
pub const PARAMS_DIR: &str = "./zkevm/test_params";
pub const SEED_PATH: &str = "./zkevm/test_seed";

pub static ENV_LOGGER: Once = Once::new();

use once_cell::sync::Lazy;
pub static CIRCUIT: Lazy<String> = Lazy::new(|| read_env_var("CIRCUIT", "super".to_string()));

pub fn init() {
    ENV_LOGGER.call_once(|| {
        dotenv::dotenv().ok();
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();
        log::info!("git version {}", GIT_VERSION);
    });
}

pub fn load_batch_traces(batch_dir: &str) -> (Vec<String>, Vec<types::eth::BlockTrace>) {
    let file_names: Vec<String> = glob(&format!("{batch_dir}/**/*.json"))
        .unwrap()
        .map(|p| p.unwrap().to_str().unwrap().to_string())
        .collect();
    log::info!("test batch with {:?}", file_names);
    let mut names_and_traces = file_names
        .into_iter()
        .map(|trace_path| {
            let trace: BlockTrace = get_block_trace_from_file(trace_path.clone());
            (
                trace_path,
                trace.clone(),
                trace.header.number.unwrap().as_u64(),
            )
        })
        .collect::<Vec<_>>();
    names_and_traces.sort_by(|a, b| a.2.cmp(&b.2));
    log::info!(
        "sorted: {:?}",
        names_and_traces
            .iter()
            .map(|(f, _, _)| f.clone())
            .collect::<Vec<String>>()
    );
    names_and_traces.into_iter().map(|(f, t, _)| (f, t)).unzip()
}

pub fn parse_trace_path_from_mode(mode: &str) -> &'static str {
    let trace_path = match mode {
        "empty" => "./zkevm/tests/traces/empty.json",
        "greeter" => "./zkevm/tests/traces/greeter.json",
        "single" => "./zkevm/tests/traces/erc20/single.json",
        "multiple" => "./zkevm/tests/traces/erc20/multiple.json",
        "native" => "./zkevm/tests/traces/native_transfer.json",
        "dao" => "./zkevm/tests/traces/dao/propose.json",
        "nft" => "./zkevm/tests/traces/nft/mint.json",
        "sushi" => "./zkevm/tests/traces/sushi/chef_withdraw.json",
        _ => "./zkevm/tests/traces/erc20/multiple.json",
    };
    log::info!("using mode {:?}, testing with {:?}", mode, trace_path);
    trace_path
}

pub fn load_block_traces_for_test() -> (Vec<String>, Vec<BlockTrace>) {
    let trace_path: String = read_env_var("TRACE_PATH", "".to_string());
    let paths: Vec<String> = if trace_path.is_empty() {
        // use mode
        let mode = read_env_var("MODE", "multiple".to_string());
        if mode.to_lowercase() == "batch" || mode.to_lowercase() == "pack" {
            (1..=10)
                .map(|i| format!("zkevm/tests/traces/bridge/{:02}.json", i))
                .collect()
        } else {
            vec![parse_trace_path_from_mode(&mode).to_string()]
        }
    } else if !std::fs::metadata(&trace_path).unwrap().is_dir() {
        vec![trace_path]
    } else {
        load_batch_traces(&trace_path).0
    };
    log::info!("test cases traces: {:?}", paths);
    let traces: Vec<_> = paths.iter().map(get_block_trace_from_file).collect();
    (paths, traces)
}

fn test_target_circuit_prove_verify<C: TargetCircuit>() {
    use std::time::Instant;

    use zkevm::verifier::Verifier;

    init();
    let mut rng = XorShiftRng::from_seed([0u8; 16]);

    let (_, block_traces) = load_block_traces_for_test();

    log::info!("start generating {} proof", C::name());
    let now = Instant::now();
    let mut prover = Prover::from_fpath(PARAMS_DIR, SEED_PATH);
    let proof = prover
        .create_target_circuit_proof_batch::<C>(&block_traces, &mut rng)
        .unwrap();
    log::info!("finish generating proof, elapsed: {:?}", now.elapsed());

    let output_file = format!(
        "/tmp/{}_{}.json",
        C::name(),
        Utc::now().format("%Y%m%d_%H%M%S")
    );
    let mut fd = std::fs::File::create(&output_file).unwrap();
    serde_json::to_writer_pretty(&mut fd, &proof).unwrap();
    log::info!("write proof to {}", output_file);

    log::info!("start verifying proof");
    let now = Instant::now();
    let mut verifier = Verifier::from_fpath(PARAMS_DIR, None);
    assert!(verifier.verify_target_circuit_proof::<C>(&proof).is_ok());
    log::info!("finish verifying proof, elapsed: {:?}", now.elapsed());
}

pub fn main() {
    test_target_circuit_prove_verify::<SuperCircuit>();
}