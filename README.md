<!-- # <img src="dream_egg.png" alt="egg of dreams" height="40" align="left"> DreamEgg -->

A pre-print of Stitch is available [here](https://mlb2251.github.io/stitch_jul11.pdf).

Tutorial coming soon!

# Stitch

## Quickstart

Run `cargo run --release --bin=compress -- data/cogsci/nuts-bolts.json --max-arity=3 --iterations=10`

In less than a second this should produce an output like:

```
=======Compression Summary=======
Found 10 inventions
Cost Improvement: (11.93x better) 1919558 -> 160946
fn_0 (1.78x wrt orig): utility: 837792 | final_cost: 1079238 | 1.78x | uses: 320 | body: [fn_0 arity=2: (T (repeat (T l (M 1 0 -0.5 (/ 0.5 (tan (/ pi #1))))) #1 (M 1 (/ (* 2 pi) #1) 0 0)) (M #0 0 0 0))]
fn_1 (3.81x wrt orig): utility: 572767 | final_cost: 503538 | 2.14x | uses: 190 | body: [fn_1 arity=3: (repeat (T (T #2 (M 0.5 0 0 0)) (M 1 0 (* #1 (cos (/ pi 4))) (* #1 (sin (/ pi 4))))) #0 (M 1 (/ (* 2 pi) #0) 0 0))]
fn_2 (6.06x wrt orig): utility: 185436 | final_cost: 316890 | 1.59x | uses: 168 | body: [fn_2 arity=1: (T (T c (M 2 0 0 0)) (M #0 0 0 0))]
fn_3 (7.18x wrt orig): utility: 48984 | final_cost: 267198 | 1.19x | uses: 82 | body: [fn_3 arity=2: (C #1 (T r (M #0 0 0 0)))]
fn_4 (8.29x wrt orig): utility: 35046 | final_cost: 231646 | 1.15x | uses: 88 | body: [fn_4 arity=2: (C (fn_0 4 #1) (fn_0 #0 6))]
fn_5 (9.04x wrt orig): utility: 18885 | final_cost: 212456 | 1.09x | uses: 95 | body: [fn_5 arity=3: (C #2 (fn_1 #1 1.5 #0))]
fn_6 (9.93x wrt orig): utility: 18885 | final_cost: 193266 | 1.10x | uses: 95 | body: [fn_6 arity=3: (C #2 (fn_1 #1 3 #0))]
fn_7 (10.53x wrt orig): utility: 10604 | final_cost: 182358 | 1.06x | uses: 54 | body: [fn_7 arity=2: (C #1 (fn_0 #0 6))]
fn_8 (11.20x wrt orig): utility: 10503 | final_cost: 171450 | 1.06x | uses: 36 | body: [fn_8 arity=2: (C (fn_0 4 #1) (fn_2 #0))]
fn_9 (11.93x wrt orig): utility: 10202 | final_cost: 160946 | 1.07x | uses: 52 | body: [fn_9 arity=0: (fn_4 4.25 6)]
Time: 227ms
```

Brief guide to reading this:

- `fn_0` is the autogenerated name of the abstraction
- `(1.78x wrt orig)` means the resulting compressed programs using `inv0` were 1.78x smaller than the original programs, while later on in the line the other `1.78x` is the compression relative to the previous step (for the first step they are the same).
- `utility: 836528` this is a measure of how many much smaller the program got after rewriting it in terms of the new primitives (divide by 100 to get the approximate number of primitives that were removed)
- `uses: 320` the abstraction was useful in 320 places in the set of programs
- Note that in these abstraction `#i` is used for abstraction variables and `$i` for original program variables.

## Common command-line arguments

- `--max-arity=2` or `-a2` controls max arity of abstraction found (default is 2)
- `--iterations=10` or `-i10` controls how many iterations of compression to run. Each iteration produces one abstraction (which can build on the previous ones)
- `--threads=10` or `-t10` is a quick way to boost performance by multithreading (default is 1)


## All command-line arguments
From `cargo run --release --bin=compress -- --help`
```
ARGS:
    <FILE>    json file to read compression input programs from

OPTIONS:
    -a, --max-arity <MAX_ARITY>
            max arity of abstractions to find (will find all from 0 to this number inclusive)
            [default: 2]

        --args-from-json
            extracts argument values from the json; specifically assumes a key value pair like
            "stitch_args": "data/dc/logo_iteration_1_stitchargs.json -a3 -t8 --fmt=dreamcoder
            --dreamcoder-drop-last --no-mismatch-check", in the toplevel dictionary of the json. All
            other commandline args get discarded when you specify this option

    -b, --batch <BATCH>
            how many worklist items a thread will take at once [default: 1]

        --dreamcoder-comparison
            anything related to running a dreamcoder comparison

        --dynamic-batch
            threads will autoadjust how large their batches are based on the worklist size

        --fmt <FMT>
            the format of the input file, e.g. 'programs-list' for a simple JSON array of programs
            or 'dreamcoder' for a JSON in the style expected by the original dreamcoder codebase.
            See [formats.rs] for options or to add new ones [default: programs-list] [possible
            values: dreamcoder, programs-list, split-programs-list]

        --follow-track
            for debugging: prunes all branches except the one that leads to the `--track`
            abstraction

    -h, --help
            Print help information

        --hole-choice <HOLE_CHOICE>
            Method for choosing hole to expand at each step, doesn't have a huge effect [default:
            depth-first] [possible values: random, breadth-first, depth-first, max-largest-subset,
            high-entropy, low-entropy, max-cost, min-cost, many-groups, few-groups, few-apps]

    -i, --iterations <ITERATIONS>
            Number of iterations to run compression for (number of inventions to find) [default: 3]

    -n, --inv-candidates <INV_CANDIDATES>
            Number of invention candidates compression_step should return in a *single* step. Note
            that these will be the top n optimal candidates modulo subsumption pruning (and the top-
            1  is guaranteed to be globally optimal) [default: 1]

        --no-mismatch-check
            disables the safety check for the utility being correct; you only want to do this if you
            truly dont mind unsoundness for a minute

        --no-opt
            disable all optimizations

        --no-opt-arity-zero
            disable the arity zero priming optimization

        --no-opt-force-multiuse
            disable the force multiuse pruning optimization

        --no-opt-free-vars
            disable the free variable pruning optimization

        --no-opt-single-task
            disable the single task pruning optimization

        --no-opt-single-use
            disable the single structurally hashed subtree match pruning

        --no-opt-upper-bound
            disable the upper bound pruning optimization

        --no-opt-useless-abstract
            disable the useless abstraction pruning optimization

        --no-other-util
            makes it so utility is based purely on corpus size without adding in the abstraction
            size

        --no-stats
            Disable stat logging - note that stat logging in multithreading requires taking a mutex
            so it can be a source of slowdown in the massively multithreaded case, hence this flag
            to disable it

        --no-top-lambda
            makes it so inventions cant start with a lambda at the top

    -o, --out <OUT>
            json output file [default: out/out.json]

        --print-stats <PRINT_STATS>
            print stats this often (0 means never) [default: 0]

    -r, --show-rewritten
            print out programs rewritten under abstraction

        --rewrite-check
            whenever you finish an invention do a full rewrite to check that rewriting doesnt raise
            a cost mismatch exception

        --save-rewritten <SAVE_REWRITTEN>
            saves the rewritten frontiers in an input-readable format

        --shuffle
            shuffle order of set of inventions

    -t, --threads <THREADS>
            number of threads (no parallelism if set to 1) [default: 1]

        --track <TRACK>
            for debugging: pattern or abstraction to track

        --truncate <TRUNCATE>
            truncate set of inventions to include only this many (happens after shuffle if shuffle
            is also specified)

        --utility-by-rewrite
            calculate utility exhaustively by performing a full rewrite; mainly used when cost
            mismatches are happening and we need something slow but accurate

        --verbose-best
            prints whenever a new best abstraction is found

        --verbose-worklist
            prints every worklist item as it is processed (will slow things down a ton due to
            rendering out expressins)
```
## Disabling optimizations

`cargo run --release --bin=compress -- data/cogsci/nuts-bolts.json --no-opt`

Or see the other commandline arguments beginning with `--no-opt-` to disable specific optimizations

## Python Bindings

Currently initial Python bindings are offered.
- Build the bindings by running `./gen_bindings.sh` (they will be added to `bindings/`)
  - Tell me or open an issue if this command doesn't work! It may vary by OS and the current command may be somewhat OSX-specific in the `rustc` flags used but that could be improved
- Add the `stitch/bindings/` folder to your `$PYTHONPATH`, for example by adding `export PYTHONPATH="$PYTHONPATH:path/to/stitch/bindings/"` to your  `~/.bashrc` or however you do it with your particular shell / venv. This will mean the `stitch.so` file is in your python path which will let you import it.
- Launch `python` and try to `import stitch`.
- As a simple example run the Python code `import stitch,json; result = json.loads(stitch.compression(["(a a a)", "(b b b)"], iterations=1, max_arity=2)); print("Result:", result)` and it should find the `(#0 #0 #0)` abstraction.
- There are a lot more keyword arguments available (full list in `examples/stitch.rs` which is where the bindings live since keeping things in `examples/` is a workaround for having a project generate a cdylib for Python bindings in addition to normal Rust/). Basically everything that you would find in  `cargo run --release --bin=compress -- --help` is included.


<!--
## Benchmarking

* `cargo bench` runs the benchmarks. Running it twice in a row (e.g. from different branches) will do a comparison

Comparing your feature before merging it:

```bash
git checkout main
cargo bench
git checkout feature
cargo bench
```

<!-- ```bash
git checkout main
cargo bench --bench=compress_bench -- --save-baseline=main
git checkout feature
cargo bench --bench=compress_bench -- --save-baseline=feature
cargo bench -- --load-baseline=feature --baseline=master
``` -->

Details:

- `--save-baseline=main` saves a named baseline (comparing against a past version of it if it exists, then overwriting it)
- `--load-baseline=feature` means *don't run any benchmarks* just load the file as if it's a result that you just produced
- `--baseline=master` overrides which benchmark we're going to compare against
- `--bench=compress_bench` avoids the "unrecognized option" error detailed [here](https://bheisler.github.io/criterion.rs/book/faq.html#cargo-bench-gives-unrecognized-option-errors-for-valid-command-line-options)

-->

## Flamegraph

install if you havent: `cargo install flamegraph`
`cargo flamegraph --root --open --deterministic --output=out/flamegraph.svg --bin=compress -- data/cogsci/nuts-bolts.json`

## Acknowledgement

This work is supported in part by the Defense Advanced Research Projects Agency (DARPA) under the program Symbiotic Design for Cyber Physical Systems (SDCPS) Contract FA8750-20-C-0542 (Systemic Generative Engineering). The views, opinions, and/or findings expressed are those of the author(s) and do not necessarily reflect the view of DARPA.
