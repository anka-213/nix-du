// SPDX-License-Identifier: LGPL-3.0

#[macro_use]
extern crate clap;
extern crate human_size;
extern crate petgraph;

pub mod depgraph;
pub mod dot;
pub mod reduction;
pub mod bindings;
use std::io;
use human_size::Size;

/* so that these functions are available in libnix_adepter.a */
pub use depgraph::{register_node, register_edge};

fn main() {
    let matches = clap::App::new("nix-du")
        .about(
            "visualise what gc-roots you should delete to free space in your nix-store",
        )
        .long_about(
            "
This program outputs a graph on stdout in the dot format which may help you figuring out which
gc-roots should be removed in order to reclaim space in the nix store.

To get started, if you are interested in freeing, say, 500MB, run
`nix-du -s 500MB | tred | dot -Tsvg > /tmp/blah.svg`
and then view the result in a browser or dedicated software like zgrviewer.

Without options, `nix-du` outputs a graph where all nodes on which the same set of
gc-roots depend are coalesced into one. The resulting node has the size of the sum,
and the label of an arbitrary component. An arrow from A to B means that while A is
alive, B is also alive.

As a rule of thumb, a node labeled `foo, 30KB` means that if you remove enough roots to get rid of
this node, then you will free `30KB`. The label `foo` may or may not have a meaning.

With some options, you can filter out some more nodes to make the graph more readable. Note
that gc-roots which don't match such filters but have a filtered-in child are kept.

The graph can be further simplified by piping it to `tred` (transitive reduction) which is usually
provided as part of graphviz. This is strongly recommmended.
",
        )
        .version(crate_version!())
        .arg(
            clap::Arg::with_name("min-size")
                .short("s")
                .long("min-size")
                .value_name("SIZE")
                .help(
                    "Hide nodes below this size (a unit should be specified: -s=50MB)",
                )
                .takes_value(true),
        )
        .arg(
            clap::Arg::with_name("nodes")
                .short("n")
                .long("nodes")
                .value_name("N")
                .conflicts_with("min-size")
                .help(
                    "Only keep the approximately N biggest nodes",
                )
                .takes_value(true),
        )
        .get_matches();

    let mut min_size = match matches.value_of("min-size") {
        Some(min_size_str) => {
            min_size_str
                .parse::<Size>()
                .unwrap_or_else(|_| {
                    clap::Error::value_validation_auto(
    "The argument to --min-size is not a valid syntax. Try -s=5MB for example."
    .to_owned()).exit()
                })
                .into_bytes() as u64
        }
        None => 0,
    };
    let n_nodes = match matches.value_of("nodes") {
        Some(min_size_str) => {
            match min_size_str.parse::<usize>() {
                Ok(x) if x > 0 => x,
                _ => {
                    clap::Error::value_validation_auto(
                        "The argument to --nodes is not a positive integer".to_owned(),
                    ).exit()
                }
            }
        }
        None => 0,
    };

    eprint!("Reading dependency graph from store... ");
    let mut g = depgraph::DepInfos::read_from_store().unwrap_or_else(|res| {
        eprintln!("Could not read from store");
        std::process::exit(res)
    });
    eprintln!(
        "{} nodes, {} edges read.",
        g.graph.node_count(),
        g.graph.edge_count()
    );
    g = reduction::merge_transient_roots(g);
    eprint!("Computing quotient graph... ");
    g = reduction::condense(g);
    eprintln!(
        "{} nodes, {} edges",
        g.graph.node_count(),
        g.graph.edge_count()
    );

    if n_nodes > 0 && n_nodes < g.graph.node_count() {
        let mut sizes: Vec<u64> = g.graph.raw_nodes().iter().map(|n| n.weight.size).collect();
        sizes.sort_unstable();
        min_size = sizes[sizes.len().saturating_sub(n_nodes)];
    }

    if min_size > 0 {
        g = reduction::keep(g, |d: &depgraph::Derivation| d.size >= min_size);
    }

    {
        let stdout = io::stdout();
        let mut handle = stdout.lock();
        dot::render(&g, &mut handle).expect("Cannot write to stdout");
    }
}
