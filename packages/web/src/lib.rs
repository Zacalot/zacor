pub mod extract;
pub mod fetch;
pub mod search;
mod types;

zacor_package::include_args!();

pub use search::{
    DEFAULT_ENGINE, DuckDuckGoHtmlEngine, DuckDuckGoLiteEngine, EngineRegistry, SearchEngine,
    parse_fallbacks, request_from_args as search_request_from_args, run as run_search,
    search as search_request, search_with_registry,
};
pub use types::{
    EngineFailure, FetchRecord, FetchRequest, SearchMode as Mode, SearchRequest as Request,
    SearchResult as ResultRow,
};

pub fn run_fetch(args: args::FetchArgs) -> Result<FetchRecord, String> {
    fetch::run(args)
}

pub fn run_extract(args: args::ExtractArgs) -> Result<(), String> {
    extract::run(args)
}
