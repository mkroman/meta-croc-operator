use clap::Parser;

#[derive(Parser, Debug)]
#[clap(author, about, version)]
pub struct Opts {
    /// The namespace the controller runs jobs in.
    #[clap(short, long, default_value = "meta")]
    pub namespace: String,
}
