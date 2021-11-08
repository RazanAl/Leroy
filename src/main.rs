use egg::{rewrite as rw, *};
use std::collections::{HashSet,HashMap};
use std::path::PathBuf;
use chrono;
use serde_json;
extern crate log;
use clap::Parser;
use serde_json::de::from_reader;
use std::fs::File;


/// egg dream
#[derive(Parser, Debug)]
#[clap(name = "Dream Egg")]
struct Args {
    /// json file to read compression input programs from
    #[clap(short, long, parse(from_os_str), default_value = "data/train_19.json")]
    file: PathBuf,

    /// Number of iterations to run compression for
    #[clap(short, long, default_value = "20")]
    iterations: usize,

    /// max arity of inventions
    #[clap(short='a', long, default_value = "2")]
    max_arity: usize,

    /// beam size
    #[clap(short, long, default_value = "10000000")]
    beam_size: usize,

    /// whether to print out the found inventions
    #[clap(long)]
    no_print_inventions: bool,

    /// whether to render the inventions
    #[clap(long)]
    render_inventions: bool,

    /// render the final egraph
    #[clap(long)]
    render_final: bool,

    /// render initial egraph
    #[clap(long)]
    render_initial: bool,

    /// number of inventions to extract
    #[clap(long, default_value="5")]
    num_inventions: usize,
}

const COST_NONTERMINAL: i32 = 1;
const COST_TERMINAL: i32 = 100;


define_language! {
    enum Lambda {
        Var(i32), // db index
        "app" = App([Id; 2]), // f, x
        "lam" = Lam([Id; 1]), // body
        Prim(egg::Symbol), // fallback, parses prims
        "programs" = Programs(Vec<Id>),
    }
}

type EGraph = egg::EGraph<Lambda, LambdaAnalysis>;

#[derive(Default)]
struct LambdaAnalysis;

#[derive(Debug)]
struct Data {
    upward_refs: HashSet<i32>, // "how much higher"
    inventionless_cost: i32,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
struct Invention {
    body:Id,
    arity: usize
}

impl Invention {
    fn new(body:Id, arity: usize) -> Invention {
        Invention { body, arity }
    }
    fn canonicalize(&mut self, egraph: &EGraph) {
        self.body = egraph.find(self.body);
    }
    fn is_canonical(&self, egraph: &mut EGraph) -> bool {
        self.body == egraph.find(self.body)
    }
    fn valid_invention(&self, egraph: &EGraph) -> bool {
        // even invalid Inventions are important as parts of AppLams that will propagate recursively upward,
        //  This checks that there aren't any upward refs that go beyond the args of the AppLam itself
        egraph[self.body].data.upward_refs.iter().all(|i| *i < (self.arity as i32))
    }
    fn to_expr(&self, egraph: &EGraph) -> RecExpr<Lambda> {
        let mut expr = extract(self.body, &egraph).to_string();
        for _ in 0..self.arity {
            expr = format!("(lam {})", expr);
        }
        expr.parse().unwrap()
    }
}

#[derive(Debug, Clone)]
struct AppLam {
    inv: Invention,
    args: Vec<Id>,
}

impl AppLam {
    fn new(body: Id, args: Vec<Id>) -> AppLam {
        AppLam {
            inv: Invention::new(body, args.len()),
            args: args,
        }
    }
    fn canonicalize(&mut self, egraph: &mut EGraph) {
        self.inv.canonicalize(egraph);
        for arg in &mut self.args {
            if !canonical(arg, egraph) {
                *arg = egraph.find(*arg);
            }
        }
    }
    fn is_canonical(&self, egraph: &mut EGraph) -> bool {
        self.inv.is_canonical(egraph) &&
        self.args.iter().all(|arg| canonical(arg, egraph))
    }
}

#[derive(Debug)]
struct BestInventions {
    inventionless_cost: i32,
    // (applam_body_eclass,arity) -> cost
    inventionful_cost: HashMap<Invention, i32>,
}

impl BestInventions {
    fn new(inventionless_cost: i32) -> BestInventions {
        BestInventions {
            inventionless_cost: inventionless_cost,
            inventionful_cost: HashMap::new()
        }
    }
    fn cost_under_inv(&self, inv: &Invention) -> i32 {
        // cost under invention else default cost
        // invention is (applam_body_eclass,arity)
        if self.inventionful_cost.contains_key(inv) {
            self.inventionful_cost[inv]
        } else {
            self.inventionless_cost
        }
    }
    fn new_cost_under_inv(&mut self, inv: Invention, cost:i32) {
        // in this algorithm I don't think we should ever insert a key
        // we've already inserted
        if cost < self.inventionless_cost {
            if !self.inventionful_cost.contains_key(&inv)
               || cost < self.inventionful_cost[&inv]  {
                self.inventionful_cost.insert(inv, cost);
            }
        }
    }
    fn top_inventions(&self) -> Vec<Invention> {
        let mut top_inventions: Vec<Invention> = self.inventionful_cost.keys().cloned().collect();
        top_inventions.sort_by(|a,b| self.inventionful_cost[a].cmp(&self.inventionful_cost[b]));
        top_inventions
    }
}


fn extract(eclass: Id, egraph: &EGraph) -> RecExpr<Lambda> {
    let mut extractor = Extractor::new(&egraph, ProgramCost{});
    let (_,p) = extractor.find_best(eclass);
    p
}

fn extract_enode(enode: &Lambda, egraph: &EGraph) -> RecExpr<Lambda> {
    match enode {
        Lambda::Prim(p) => {format!("{}",p)},
        Lambda::Var(i) => {format!("{}",i)},
        Lambda::App([f,x]) => {format!("(app {} {})",extract(*f,egraph),extract(*x,egraph))},
        Lambda::Lam([b]) => {format!("(lam {})",extract(*b,egraph))},
        _ => {format!("not rendered")},
    }.parse().unwrap()
}

fn extract_under_inv(
    eclass: Id,
    inv: Invention,
    applams_of_treenode: &HashMap<Id,Vec<AppLam>>,
    best_inventions_of_treenode: &HashMap<Id,BestInventions>,
    egraph: &EGraph
) -> RecExpr<Lambda> {
    let mut expr: RecExpr<Lambda> = Default::default();
    extract_under_inv_rec(eclass, inv, applams_of_treenode, best_inventions_of_treenode, egraph, &mut expr);
    expr
}

fn extract_under_inv_rec(
    root: Id,
    inv: Invention,
    applams_of_treenode: &HashMap<Id,Vec<AppLam>>,
    best_inventions_of_treenode: &HashMap<Id,BestInventions>,
    egraph: &EGraph,
    into_expr: &mut RecExpr<Lambda>,
) -> Id {
    let root = egraph.find(root);
    let target_cost:i32 = best_inventions_of_treenode[&root].cost_under_inv(&inv);

    if best_inventions_of_treenode[&root].inventionful_cost.contains_key(&inv)
       && applams_of_treenode[&root].iter().any(|applam| applam.inv == inv) {
        let applam: Vec<AppLam> = applams_of_treenode[&root].iter().filter(|applam| applam.inv == inv).cloned().collect();
        assert!(applam.len() == 1);
        let applam = &applam[0];
        let mut id: Id = into_expr.add(Lambda::Prim("inv".into()));
        // wrap the new primitive in app() calls. Note that you pass in the $0 args LAST given how appapplamlam works
        for arg in applam.args.iter().rev() {
            let arg_id = extract_under_inv_rec(*arg, inv, applams_of_treenode, best_inventions_of_treenode, egraph, into_expr);
            id = into_expr.add(Lambda::App([id,arg_id]));
        }
        assert_eq!(target_cost,cost_rec(&into_expr));
        return id
    }
    
    assert!(egraph[root].nodes.len() == 1);
    let id: Id = match &egraph[root].nodes[0] {
        Lambda::Prim(p) => {
            into_expr.add(Lambda::Prim(*p))
        },
        Lambda::Var(i) => {
            into_expr.add(Lambda::Var(*i))
        },
        Lambda::App([f,x]) => {
            let f_id = extract_under_inv_rec(*f, inv, applams_of_treenode, best_inventions_of_treenode, egraph, into_expr);
            let x_id = extract_under_inv_rec(*x, inv, applams_of_treenode, best_inventions_of_treenode, egraph, into_expr);
            into_expr.add(Lambda::App([f_id,x_id]))
        },
        Lambda::Lam([b]) => {
            let b_id = extract_under_inv_rec(*b, inv, applams_of_treenode, best_inventions_of_treenode, egraph, into_expr);
            into_expr.add(Lambda::Lam([b_id]))
        }
        Lambda::Programs(roots) => {
            let root_ids: Vec<Id> = roots.iter()
                .map(|r| extract_under_inv_rec(*r, inv, applams_of_treenode, best_inventions_of_treenode, egraph, into_expr))
                .collect();
            into_expr.add(Lambda::Programs(root_ids))
        }
    };

    assert_eq!(target_cost,cost_rec(&into_expr));
    return id
}


#[inline]
fn canonical(id:&Id, egraph: &EGraph) -> bool {
    egraph.find(*id) == *id
}

fn narrow_beam(beam: &mut HashMap<Invention,i32>, beam_size: usize) {
    if beam.len() < beam_size {
        return
    }
    // println!("Need to narrow beam! (worth turning this print message off if it ever actually prints)");
    let num_to_drop = beam_size - beam.len();
    let mut costs: Vec<(Invention,i32)> = beam.iter().map(|(id,cost)|(*id,*cost)).collect();
    // DECREASING order of cost (since i do cost2.cmp(cost1))
    costs.sort_by(|(_,cost1),(_,cost2)| cost2.cmp(cost1));
    for (id,_) in costs.iter().take(num_to_drop) {
        beam.remove(id);
    }
}

impl Analysis<Lambda> for LambdaAnalysis {
    type Data = Data;
    fn merge(&self, to: &mut Data, from: Data) -> bool {

        assert_eq!(to.upward_refs,from.upward_refs);
        // we really shouldnt be merging anyone ever rn, but later we'll want to do this
        assert_eq!(to.inventionless_cost,from.inventionless_cost);

        // keep the lowest inventionless cost
        // modified |= merge_inventionless(&mut to.inventionless_cost_any, &from.inventionless_cost_any);
        
        false // didnt modify anything
    }

    fn make(egraph: &EGraph, enode: &Lambda) -> Data {
        let mut upward_refs: HashSet<i32> = HashSet::new();
        match enode {
            Lambda::Var(i) => {
                upward_refs.insert(*i);
            }
            Lambda::Prim(_) => {
            }
            Lambda::App([f, x]) => {
                // union of f and x
                upward_refs.extend(egraph[*f].data.upward_refs.iter());
                upward_refs.extend(egraph[*x].data.upward_refs.iter());
            }
            Lambda::Lam([b]) => {
                // body, subtract 1 from all values, remove the -1 if its in there
                upward_refs.extend(egraph[*b].data.upward_refs.iter()
                    .map(|x| x-1)
                    .filter(|x| *x >= 0));
            }
            Lambda::Programs(programs) => {
                // assert no free variables in programs
                assert!(programs.iter().all(|p| egraph[*p].data.upward_refs.is_empty()));
            }
        }
        let inventionless_cost = match enode {
            Lambda::Var(_) | Lambda::Prim(_) => COST_TERMINAL,
            Lambda::App([f,x]) => {
                    COST_NONTERMINAL
                    + egraph[*f].data.inventionless_cost
                    + egraph[*x].data.inventionless_cost
                }
            Lambda::Lam([b]) => {
                COST_NONTERMINAL + egraph[*b].data.inventionless_cost
            }
            Lambda::Programs(ps) => {
                ps.iter().map(|p| egraph[*p].data.inventionless_cost).sum()
            }
        };
        Data {
               upward_refs: upward_refs,
               inventionless_cost: inventionless_cost
            }
    }

    fn modify(egraph: &mut EGraph, id: Id) {
    }
}

fn var(s: &str) -> Var {
    s.parse().unwrap()
}

fn shift(e: Id, incr_by: i32, egraph: &mut EGraph, seen: &mut HashMap<(Id,i32),Option<Id>>) -> Option<Id> {
    recursive_var_mod(
        |actual_idx, depth, which_upward_ref, egraph| {
            // if actual_idx + incr_by >= ARGC {
            //     return None // $3+ get pruned
            // } 
            Some(egraph.add(Lambda::Var(actual_idx + incr_by)))
        },
        e,egraph,seen
    )
}

// not used in the new verison but should be compatible with everything we've got!
fn inline(e: Id, replace_with: Id, egraph: &mut EGraph, seen: &mut HashMap<(Id,i32),Option<Id>>) -> Option<Id> {
    recursive_var_mod(
        |actual_idx, depth, which_upward_ref, egraph| {
            if which_upward_ref == 0 {
                // ShifterVM { incr_by: depth }.recursive_var_mod(replace_with, egraph)
                shift(replace_with, depth, egraph, &mut HashMap::new()) // note i have it just make a new hashmap on the spot for this, caching would be better
            } else {
                // we need to decrement this by 1 since its a pointer above the lambda we removed
                Some(egraph.add(Lambda::Var(actual_idx - 1)))
            }
        },
        e,egraph, seen
    )
}

fn recursive_var_mod(
    var_mod: impl Fn(i32, i32, i32, &mut EGraph) -> Option<Id>,
    eclass:Id,
    egraph: &mut EGraph,
    seen: &mut HashMap<(Id,i32),Option<Id>>
    ) -> Option<Id>
    {
        recursive_var_mod_helper(
            &var_mod,
            eclass,
            0,
            egraph,
            seen,
        )
}

fn recursive_var_mod_helper(
    var_mod: &impl Fn(i32, i32, i32, &mut EGraph) -> Option<Id>,
    eclass:Id,
    depth: i32,
    egraph: &mut EGraph,
    seen : &mut HashMap<(Id,i32),Option<Id>>,
    ) -> Option<Id>
    {
        // important invariant: a $i with i==depth would be a $0 pointer at the top level
        // meaning i<depth is an internal pointer that doesnt break the top level
        let eclass = egraph.find(eclass);
        let key = (eclass,depth);

        if seen.contains_key(&key) {
            return seen[&key];
        }
        
        if egraph[eclass].data.upward_refs.iter().all(|i| *i < depth) {
            // from our invariant (above) we know i<depth is an internal pointer that doesnt point out of the top level
            seen.insert(key, Some(eclass));
            return Some(eclass)
        }

        // this is for loop breaking (though there shouldnt be loops in my new DAG setup anyways)
        seen.insert(key, None);
        
        // if you want a multiple-node-per-eclass version of this that unions together the stuff from diff branches, see my old code!
        assert!(egraph[eclass].nodes.len() == 1);
        // clone to appease the borrow checker
        let enode = egraph[eclass].nodes[0].clone();
        let new_eclass = match enode {
            Lambda::Var(i) => {
                assert!(i >= depth); // otherwise we should have returned earlier
                // by our invariant be have i-depth as the toplevel version of this index
                var_mod(i, depth, i-depth, egraph)
            }
            Lambda::Prim(_) => {
                panic!("unreachable, Prim never has free vars")
            }
            Lambda::App([f, x]) => {
                // recurse in each (class shift will return early if no shifting is needed) and build a new App
                let fnew_opt = recursive_var_mod_helper(var_mod, f, depth, egraph, seen);
                let xnew_opt = recursive_var_mod_helper(var_mod, x, depth, egraph, seen);
                match (fnew_opt,xnew_opt) {
                    (Some(fnew),Some(xnew)) => Some(egraph.add(Lambda::App([fnew, xnew]))),
                    _ => None,
                }
            }
            Lambda::Lam([b]) => {
                // increment depth
                recursive_var_mod_helper(var_mod, b, depth+1, egraph, seen)
                .map(|bnew| egraph.add(Lambda::Lam([bnew])))
            }
            Lambda::Programs(_) => {
                panic!("attempted to shift a Programs node")
            }
        };

        if let Some(new_eclass) = new_eclass {
            let new_eclass = egraph.find(new_eclass);
            seen.insert(key, Some(new_eclass));
            Some(new_eclass)
        } else {
            None
        }
}


struct ProgramCost {}

impl CostFunction<Lambda> for ProgramCost {
    type Cost = i32;
    fn cost<C>(&mut self, enode: &Lambda, mut costs: C) -> Self::Cost
    where
        C: FnMut(Id) -> Self::Cost
    {
        match enode {
            Lambda::Var(_) | Lambda::Prim(_) => COST_TERMINAL,
            Lambda::App([f, x]) => {
                COST_NONTERMINAL + costs(*f) + costs(*x)
            }
            Lambda::Lam([b]) => {
                COST_NONTERMINAL + costs(*b)
            }
            Lambda::Programs(ps) => {
                ps.iter()
                .map(|p|costs(*p))
                .sum()
            }
        }
    }
}

fn cost_rec(expr: &RecExpr<Lambda>) -> i32 {
    ProgramCost{}.cost_rec(expr)
}

fn timestamp() -> String {
    format!("{}", chrono::Local::now().format("%Y-%m-%d_%H-%M-%S"))
}

/// finds everywhere the rewrite rules matches and applies it to each of them
/// and rebuilds the egraph. Will only apply to matches that are visible before
/// any rewriting occurs. This is the same as running a runner with an iter limit of 1.
/// I guess I'm not using this in the code right now bc I like the runner's report.
fn apply_everywhere_once(rules_: &[&str], egraph: &mut EGraph) {
    let rules: Vec<Rewrite<Lambda,LambdaAnalysis>> = rules(rules_);
    let matches: Vec<Vec<SearchMatches>> = rules.iter().map(|r| r.search(egraph)).collect();
    for (r,m) in rules.iter().zip(matches) {
        let hits = r.apply(egraph, &m).len();
        println!("(applied {} {} times out of {} matches)",r.name(),hits, m.len());
    }
    egraph.rebuild();
}

fn saturate(rules_: &[&str], render: bool, out_dir: String, egraph: EGraph) -> EGraph {
    let rules: Vec<Rewrite<Lambda,LambdaAnalysis>> = rules(rules_);
    let mut runner = Runner::default()
        .with_egraph(egraph)
        .with_iter_limit(400)
        .with_scheduler(SimpleScheduler)
        .with_time_limit(core::time::Duration::from_secs(200))
        .with_node_limit(3000000);
    
    if render {
        runner = runner.with_hook(
        {
            let out_dir = out_dir.clone(); // silly thing to clone into the closure
            move |runner|{
                let iter = runner.iterations.len();
                println!("Iter {}: {}", iter, egraph_info(&runner.egraph));
                save(&runner.egraph, format!("3_propagate_{}",iter).as_str(), &out_dir);
                Ok(())
            }
        });
    }

    let runner = runner.run(rules.iter());
    runner.print_report();
    runner.egraph
}

fn run_pretty(rule_: &str, name:&str, egraph: &mut EGraph) {
    let rule: Rewrite<Lambda,LambdaAnalysis> = rule(rule_);
    let matches = rule.search(egraph);
    egraph.dot().to_png(format!("target/match_{}_0pre.png",name)).unwrap();
    rule.apply(egraph, &matches).len();
    egraph.dot().to_png(format!("target/match_{}_1post.png",name)).unwrap();
    egraph.rebuild();
    egraph.dot().to_png(format!("target/match_{}_2rebuild.png",name)).unwrap();
}

fn search(pat: &str, egraph: &EGraph) -> Vec<SearchMatches>{
    let applam:Pattern<Lambda> = pat.parse().unwrap();
    applam.search(&egraph)
}

fn save(egraph: &EGraph, name: &str, outdir: &str) {
    egraph.dot().to_png(format!("{}/{}.png",outdir,name)).unwrap();
}

fn save_expr(expr: &RecExpr<Lambda>, name: &str, outdir: &str) {
    let mut egraph: EGraph = Default::default();
    egraph.add_expr(expr);
    egraph.dot().to_png(format!("{}/{}.png",outdir,name)).unwrap();
}

fn rule_map() -> HashMap<String,Rewrite<Lambda, LambdaAnalysis>> {
    vec![
    ].into_iter().map(|r:Rewrite<Lambda, LambdaAnalysis>| (r.name().to_string(),r)).collect()
}

// ownership is a pain so this is a helper
fn rule(name: &str) -> Rewrite<Lambda, LambdaAnalysis> {
    rule_map().remove(name).expect(format!("rule {} not found",name).as_str())
}

fn rules(names: &[&str]) -> Vec<Rewrite<Lambda, LambdaAnalysis>> {
    names.iter().map(|name|rule(name)).collect()
}

fn egraph_info(egraph: &EGraph) -> String {
    format!("{} nodes, {} classes, {} memo", egraph.total_number_of_nodes(), egraph.number_of_classes(), egraph.total_size())
}

fn toplogical_ordering(root: Id, egraph: &EGraph) -> Vec<Id> {
    // returns a Vec of Ids ending in the root Id (ie child first traversal)
    // and notably an Id never shows up twice (if it was already there earlier it wont be added again)
    //todo  assumes no cycles!! AND assumes each eclass only has one enode at this point, though you could relax the latter
    let mut vec = Vec::new();
    toplogical_ordering_rec(root, egraph, &mut vec);
    vec
}

fn toplogical_ordering_rec(root: Id, egraph: &EGraph, vec: &mut Vec<Id>) {
    // assumes no cycles.
    // we require at this point that all eclasses only have ONE enode
    assert!(egraph[root].nodes.len() == 1);
    for child in egraph[root].nodes[0].children(){
        toplogical_ordering_rec(*child, egraph, vec);
    }
    if !vec.contains(&root) {
        // if we're already a child of someone else earlier we dont need to be readded
        vec.push(root);
    }
}

/// Does all the work
fn run_inversions(
    treenodes: &Vec<Id>,
    max_arity: usize,
    beam_size: usize,
    egraph: &mut EGraph
)
   -> (HashMap<Id,Vec<AppLam>>,HashMap<Id,BestInventions>) {
    // one vector of applams per tree node
    let mut applams_of_treenode: HashMap<Id,Vec<AppLam>> = Default::default();
    let mut best_inventions_of_treenode: HashMap<Id,BestInventions> = Default::default();
    
    let var0: Id = egraph.add(Lambda::Var(0));

    // init caches
    // be SUPER careful to index with arity-1 not plain arity
    let mut cache_bubble_lam: Vec<HashMap<(Id,i32),Option<Id>>> = Default::default();
    let mut cache_shift: Vec<HashMap<(Id,i32),Option<Id>>> = Default::default();
    for _ in 0..max_arity {
        cache_bubble_lam.push(Default::default());
        cache_shift.push(Default::default());
    }

    for treenode in treenodes.iter() {
        // println!("processing id={}: {}", treenode, extract(*treenode, egraph) );

        // im essentially using the egraph just for its structural hashing rn
        assert!(egraph[*treenode].nodes.len() == 1);
        // clone to appease the borrow checker
        let node = egraph[*treenode].nodes[0].clone();

        // todo maybe should just straight up call canoncialize here to make sure instead of just asserting it
        // its very very very important that these are all canonical
        debug_assert!(node.children().iter().all(|c| applams_of_treenode[c].iter().all(|applam| applam.is_canonical(egraph))));
        debug_assert!(node.children().iter().all(|c| best_inventions_of_treenode[c].inventionful_cost.keys().all(|inv| inv.is_canonical(egraph))));

        //==================================//
        // *** PROPAGATE/CREATE APPLAMS *** //
        //==================================//
        
        let mut applams: Vec<AppLam> = Vec::new();
        // any node can become the identity applam
        applams.push(AppLam::new(var0, vec![*treenode]));

        match node {
            Lambda::Var(_) | Lambda::Prim(_) | Lambda::Programs(_) => {},
            Lambda::App([f,x]) => {
                let ref f_applams = applams_of_treenode[&f];
                let ref x_applams = applams_of_treenode[&x];

                // bubbling from the left:
                // (app f x) == (app (applam body arg) x) => (applam (app body upshift(x)) arg)
                for f_applam in f_applams.iter() {
                    let arity = f_applam.inv.arity;
                    let arity_i32 = arity as i32;
                    let shifted_x = shift(x, arity_i32, egraph, &mut cache_shift[arity-1]).unwrap();
                    let new_applam_body = egraph.add(Lambda::App([f_applam.inv.body,shifted_x]));
                    applams.push(AppLam::new(new_applam_body, f_applam.args.clone()));
                }

                // bubbling from the right:
                // (app f x) == (app f (applam body arg)) => (applam (app upshift(f) body) arg)
                for x_applam in x_applams.iter() {
                    let arity = x_applam.inv.arity;
                    let arity_i32 = arity as i32;
                    let shifted_f = shift(f, arity_i32, egraph, &mut cache_shift[arity-1]).unwrap();
                    let new_applam_body = egraph.add(Lambda::App([shifted_f, x_applam.inv.body]));
                    applams.push(AppLam::new(new_applam_body, x_applam.args.clone()));
                }

                println!("f_applam x_applam pairwise product size: {} x {} -> {}",f_applams.len(), x_applams.len(), f_applams.len() * x_applams.len());

                for f_applam in f_applams.iter() {
                    for x_applam in x_applams.iter() {
                        // making a higher arity applam out of two diff applams
                        // and merging any shared arguments. Higher arity applam looks a bit like:
                        // (app f x) == (app (applam body1 arg1) (applam body2 arg2)) => (applam (app body1 upshift(body2)) arg1 arg2)
                        // note that (applam body arg0 arg1) means arg0 will fill upward refs to $0 and arg1 will fill upward refs to $1
                        // so somewhat confusingly (applam body arg0 arg1) == (app (app (lam (lam body)) arg1) arg0) - but hopefully you dont need to think about that
                        // Merging: when f_applam and x_applam have identical args we can merge them
                        // (app f x) == (app (applam body1 arg) (applam body2 arg)) => (applam (app body1 body2) arg)
                        // here we do that for partial overlap between the two as well!

                        let (shifted_x_applam_body,new_x_applam_args) = if
                            f_applam.args.iter().any(|farg| x_applam.args.contains(farg)) {
                            // merging is needed

                            // x_shift_table[1] tells us how much to shift an upward ref to $1 in x_applam.body
                            // (note without merging this would be the arity of f_applam)
                            let mut x_shift_table: Vec<i32> = vec![];
                            let mut to_remove: Vec<usize> = vec![];
                            let mut shift_rest_by = f_applam.inv.arity as i32; // normal amt we shift x by, except if there are merges to be done. If a merge happens all the higher x vars get shifted less, and the specific x var gets shifted a very specific amount
                            for (x_idx,xarg) in x_applam.args.iter().enumerate() {
                                if let Some(f_idx) = f_applam.args.iter().position(|farg| farg == xarg) {
                                    // we found a match! $x_idx should map to the same thing as $f_idx.
                                    // remember, our body currently has $x_idx at the toplevel so now
                                    // we want to shift it by $(f_idx-x_idx) so that it ends up as f_idx.
                                    x_shift_table.push((f_idx as i32) - (x_idx as i32));
                                    to_remove.push(x_idx);
                                    shift_rest_by -= 1; // effectively downshifts all the higher args now that this one is gone
                                } else {
                                    // shift fully without merging
                                    x_shift_table.push(shift_rest_by);
                                }
                            }

                            // remove the args from xargs that we can merge into fargs
                            let mut new_x_applam_args = x_applam.args.clone();
                            for x_idx in to_remove.iter().rev() {
                                new_x_applam_args.remove(*x_idx);
                            }


                            let shifted_x_applam_body = recursive_var_mod(
                                |actual_idx, _depth, which_upward_ref, egraph| {
                                    if which_upward_ref < x_applam.inv.arity as i32 {
                                        Some(egraph.add(Lambda::Var(actual_idx + x_shift_table[which_upward_ref as usize])))
                                    } else {
                                        Some(egraph.add(Lambda::Var(actual_idx)))
                                    }
                                }, x_applam.inv.body, egraph, &mut HashMap::new()).unwrap();
                            (shifted_x_applam_body, new_x_applam_args)
                        } else {
                            let shifted_x_applam_body = shift(x_applam.inv.body, f_applam.inv.arity as i32, egraph, &mut cache_shift[f_applam.inv.arity-1]).unwrap();
                            let new_x_applam_args = x_applam.args.clone();
                            (shifted_x_applam_body, new_x_applam_args)
                        };

                        // no merging needed!
                        if {f_applam.inv.arity + new_x_applam_args.len()} > max_arity {
                            continue;
                        }

                        let new_applam_body = egraph.add(Lambda::App([f_applam.inv.body,shifted_x_applam_body]));
                        let mut new_applam_args = f_applam.args.clone();
                        new_applam_args.extend(new_x_applam_args);
                        applams.push(AppLam::new(new_applam_body, new_applam_args));
                            
                    }
                    
                }

            },
            Lambda::Lam([b]) => {
                let ref b_applams = applams_of_treenode[&b];
                // bubbling up over the lambda:
                // (lam b) == (lam (applam body arg)) => (applam (lam careful_shift(body)) arg)
                // where:
                //  - arg must not have any upward refs to $0 in it since we cant jump over a lambda we point to
                //    > (in the multiarg applam case, none of them can have $0)
                //  - we need to shift the body in a very specific way. Say the applam was arity 3. Then
                //    any outgoing refs to $0 $1 $2 in the original body point to these args, and $3 points to the lam
                //    we're about to jump over. Now the lam is 3 levels deeper so pointers to $3 at the top
                //    level should now point to $0. Meanwhile pointers to $0 $1 $2 should be incremented by 1 since
                //    theres one more lambda in the way now.
                for b_applam in b_applams.iter() {
                    if b_applam.args.iter().any(|arg| egraph[*arg].data.upward_refs.contains(&0)) {
                        continue;
                    }
                    let arity = b_applam.inv.arity;
                    let arity_i32 = arity as i32;
                    let shifted_b = recursive_var_mod(
                        |actual_idx, _depth, which_upward_ref, egraph| {
                            if which_upward_ref == arity_i32 {
                                Some(egraph.add(Lambda::Var(actual_idx - arity_i32)))
                            } else if which_upward_ref < arity_i32 {
                                Some(egraph.add(Lambda::Var(actual_idx + 1)))
                            } else {
                                Some(egraph.add(Lambda::Var(actual_idx)))
                            }
                        }, b_applam.inv.body, egraph, &mut cache_bubble_lam[arity-1]).unwrap();

                    let new_applam_body = egraph.add(Lambda::Lam([shifted_b]));
                    applams.push(AppLam::new(new_applam_body, b_applam.args.clone()));
                }
            },
        }

        //===================================//
        // *** CALCULATE BEST INVENTIONS *** //
        //===================================//

        let mut best_inventions = BestInventions::new(egraph[*treenode].data.inventionless_cost);

        // replacing this node with a call to an invention
        for applam in applams.iter() {
            if applam.inv.valid_invention(egraph) && applam.inv.body != var0 {
                let cost: i32 =
                    COST_TERMINAL // the new primitive for this invention
                    + COST_NONTERMINAL * applam.inv.arity as i32 // the chain of app()s needed to apply the new primitive
                    + applam.args.iter().map(|id| best_inventions_of_treenode[&id].cost_under_inv(&applam.inv)).sum::<i32>(); // costs of actual args
                best_inventions.new_cost_under_inv(applam.inv, cost);
            }
        }
                
        // which inventions helped our children?
        let child_inventions: Vec<Invention> = node.children().iter()
            .map(|id| best_inventions_of_treenode[id].inventionful_cost.keys().cloned())
            .flatten()
            .collect();

            
        // inventions based on specific node type
        match node {
            Lambda::Var(_) | Lambda::Prim(_) => {},
            Lambda::App([f,x]) => {
                let ref f_best_inventions = best_inventions_of_treenode[&f];
                let ref x_best_inventions = best_inventions_of_treenode[&x];
                                
                // costs with inventions as 1 + fcost + xcost. Use inventionless cost as a default.
                // if either fcost or xcost is None (ie infinite)
                for inv in child_inventions {
                    let fcost = f_best_inventions.cost_under_inv(&inv);
                    let xcost = x_best_inventions.cost_under_inv(&inv);
                    let cost = COST_NONTERMINAL+fcost+xcost;
                    best_inventions.new_cost_under_inv(inv, cost);
                }
            }
            Lambda::Lam([b]) => {
                // just map +1 over the costs
                let ref b_best_inventions = best_inventions_of_treenode[&b];
                for (inv,cost) in b_best_inventions.inventionful_cost.iter() {
                    best_inventions.new_cost_under_inv(*inv, cost + COST_NONTERMINAL);
                }
            }
            Lambda::Programs(roots) => {
                // union together all the useful inventions of diff programs
                
                // count num occurences of each invention
                let mut counts: HashMap<Invention,i32> = child_inventions.iter().map(|i| (*i,0)).collect();
                for inv in child_inventions {
                    counts.insert(inv, counts[&inv] + 1);
                }

                // keep only inventions used by 2+ programs
                // (otherwise it's pretty boring and just abstracts out an entire program)
                let inventions: Vec<Invention> = counts.iter()
                    .filter_map(|(i,c)| if *c > 1 { Some(*i) } else { None }).collect();
                
                for inv in inventions {
                    let cost = roots.iter().map(|root| {
                            best_inventions_of_treenode[root].cost_under_inv(&inv)
                        }).sum();
                    best_inventions.new_cost_under_inv(inv, cost);
                }
            }
        }
        narrow_beam(&mut best_inventions.inventionful_cost, beam_size);

        applams_of_treenode.insert(*treenode, applams);
        best_inventions_of_treenode.insert(*treenode, best_inventions);

    }
    (applams_of_treenode,best_inventions_of_treenode)
}





fn main() {
    env_logger::init();

    let args: Args = Args::parse();

    // create a new directory for logging outputs
    let out_dir: String = format!("target/{}",timestamp());
    let out_dir_p = std::path::Path::new(out_dir.as_str());
    assert!(!out_dir_p.exists());
    std::fs::create_dir(out_dir_p).unwrap();

    // first dreamcoder program
    let programs: Vec<String> = from_reader(File::open(args.file).expect("file not found")).expect("json deserializing error");

    println!("Programs: {}", programs.len());
        
    // my way of combining the programs since RecExprs arent easily combinable
    let programs_expr: RecExpr<Lambda> = format!("(programs {})",programs.join(" ")).parse().unwrap();

    let mut egraph: EGraph = Default::default();
    let programs_id = egraph.add_expr(&programs_expr);
    egraph.rebuild(); // this is VERY important to run before you try applying any searches or rewrites

    let applam:Pattern<Lambda> = "(app (lam ?body) ?arg)".parse().unwrap();
    assert!(applam.search(&egraph).is_empty(),
        "Normal dreamcoder programs never have unapplied lambdas in them! 
         Who knows what might happen if you run this. Side note you can probably
         inline them and it'd be fine (we've got a function for that!) and also
         who knows maybe it wouldnt be an issue in the first place");

    println!("Initial egraph:\n\t{}\n", egraph_info(&egraph));
    if args.render_initial {
        save(&egraph, "0_programs", &out_dir);
    }

    let tstart = std::time::Instant::now();
    let treenodes: Vec<Id> = toplogical_ordering(programs_id, &egraph);
    // println!("Topological ordering:");
    // treenodes.iter().for_each(|&id| {
    //     println!("id={}: {}", id, extract(id,&egraph));
    // });

    let (applams_of_treenode,best_inventions_of_treenode)
        = run_inversions(
            &treenodes,
            args.max_arity,
            args.beam_size,
            &mut egraph
        );

    egraph.rebuild(); // hopefully doesnt matter at all anyways, not sure if we needed to do this thruout inversions

    let elapsed = tstart.elapsed().as_millis();

    println!("Inventionless (cost={:?}):\n{}\n",
        egraph[programs_id].data.inventionless_cost,
        extract(programs_id, &egraph)
    );

    let top_invs: Vec<Invention> = best_inventions_of_treenode[&programs_id].top_inventions();
    println!("Found {} Inventions that helped at the top level", top_invs.len());

    println!("\n*** Core stuff took: {}ms ***\n", elapsed);

    if !args.no_print_inventions {
        for (i,inv) in top_invs.iter().take(args.num_inventions).enumerate() {
            let inv_expr = inv.to_expr(&egraph);
            let rewritten = extract_under_inv(programs_id, *inv, &applams_of_treenode, &best_inventions_of_treenode, &egraph);
            println!("\nInvention {} {:?} (inv_cost={:?}; rewritten_cost={:?}):\n{}\n Rewritten:\n{}",
                i,
                inv,
                cost_rec(&inv_expr),
                cost_rec(&rewritten),
                inv_expr,
                rewritten,
            );
            if args.render_inventions {
                save_expr(&inv_expr, &format!("inv{}",i), &out_dir);
            }
        }
    }

    println!("Final egraph: {}",egraph_info(&egraph));
    println!("Variables used:");
    for i in 0..10 {
        println!("{}: {}", i, search(format!("({})",i).as_str(),&egraph).len());
    }

    // for (i,inv) in top_invs.iter().enumerate() {
    //     let inv_expr = inv.to_expr(&egraph).to_string();
    //     let targets =
    //     ["(app logo_FWRT (app (app logo_MULL logo_UL) 0))",
    //      "(app logo_FWRT (app (app logo_MULL logo_UL) 1))",
    //      "(app logo_FWRT (app (app logo_MULL logo_UL) 2))",
    //      "(app logo_FWRT (app (app logo_MULL logo_UL) 3))",
    //      "(app logo_FWRT (app (app logo_MULL logo_UL) 4))"];
    //     if targets.iter().any(|target|inv_expr.contains(target)) {
    //         println!("Found target: {}", inv_expr);
    //         save_expr(&inv.to_expr(&egraph), &format!("inv{}",i), &out_dir);
    //     }
    // }

    println!("\nPrograms: {}", programs.len());
    println!("Cands useful at top level: {}",top_invs.len());
    println!("*** Core stuff took: {}ms ***\n", elapsed);

    if args.render_final {
        println!("Rendering final egraph");
        save(&egraph, "final", &out_dir);
    }

    
}
