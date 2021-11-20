use super::expr::*;
use std::collections::HashMap;
use egg::*;
use std::fmt::{self, Formatter, Display, Debug};
use std::hash::Hash;


#[derive(Debug, Clone)]
pub struct DomExpr<D: Domain> {
    pub expr: Expr,
    pub evals: HashMap<(Id,Vec<Val<D>>), Val<D>>, // from (node,env) to result
}

impl<D: Domain> From<Expr> for DomExpr<D> {
    fn from(expr: Expr) -> Self {
        DomExpr {
            expr,
            evals: HashMap::new(),
        }
    }
}

impl<D: Domain> Display for DomExpr<D> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}", self.expr)
    }
}

/// Wraps a DSL function in a struct that manages currying of the arguments
/// which are fed in one at a time through .apply(). Example: the "+" primitive
/// evaluates to a CurriedFn with arity 2 and empty partial_args. The expression
/// (app + 3) evals to a CurriedFn with vec![3] as the partial_args. The expression
/// (app (app + 3) 4) will evaluate to 7 (since .apply() will fill the last argument,
/// notice that all arguments are filled, and return the result).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CurriedFn<D: Domain> {
    name: egg::Symbol,
    arity: usize,
    partial_args: Vec<Val<D>>,
}

impl<D: Domain> CurriedFn<D> {
    pub fn new(name: egg::Symbol, arity: usize) -> Self {
        Self {
            name,
            arity,
            partial_args: Vec::new(),
        }
    }
    /// Feed one more argument into the function, returning a new CurriedFn if
    /// still not all the arguments have been received. Evaluate the function
    /// if all arguments have been received.
    pub fn apply(&self, arg: &Val<D>, handle: &mut DomExpr<D>) -> Val<D> {
        let mut new_dslfn = self.clone();
        new_dslfn.partial_args.push(arg.clone());
        if new_dslfn.partial_args.len() == new_dslfn.arity {
            D::fn_of_prim(new_dslfn.name) (&new_dslfn.partial_args, handle)
        } else {
            Val::PrimFun(new_dslfn)
        }
    }
}


#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Val<D: Domain> {
    Dom(D),
    PrimFun(CurriedFn<D>), // function ptr, arity, any args that have been partially filled in
    LamClosure(Id, Vec<Val<D>>) // body, captured env
}

impl<D: Domain> Val<D> {
    pub fn unwrap_dom(&self) -> D {
        match self {
            Val::Dom(d) => d.clone(),
            _ => panic!("Val::unwrap_dom: not a domain value")
        }
    }
}
impl<D: Domain> From<D> for Val<D> {
    fn from(d: D) -> Self {
        Val::Dom(d)
    }
}


/// todo unsure if we should require PartialEq Eq Hash
/// in particular Eq is the big one since f64 doesnt implement it. But in order
/// to use HashMaps (or even search for elements in Vecs) we need it. I think
/// it's fair to require this since itll let us do a LOT more generic algorithms that
/// work with the domain semantics. And if someone really wants f64s and know that
/// they wont be NaN they can make a f64_noNaN type with From<f64> that panics on NaN
/// and impl Eq for that, then whenever constructing a value variant that uses floats
/// just do a .into() on it. So they can use f64s within their logic and just need to
/// wrap them when passing things back out to our system.
pub trait Domain: Clone + Debug + PartialEq + Eq + Hash {
    /// given a primitive's symbol return a runtime Val object. For function primitives
    /// this should return a PrimFun(CurriedFn) object.
    fn val_of_prim(_p: egg::Symbol) -> Option<Val<Self>> {
        unimplemented!()
    }
    /// given a function primitive's symbol return the function pointer
    /// you can use to call the function.
    /// Breakdown of the function type: it takes a slice of values as input (the args)
    /// along with a mut ref to an Expr (I'll refer to as a "handle") which is necessary
    /// for calling .apply(f,x). This setup with a handle guarantees we can always track
    /// when applys happen and log them in our Expr.evals, and also it's necessary for
    /// executing LamClosures in order to look up their body Id (and we wouldn't want
    /// LamClosures to carry around full Exprs because that would break the Expr.evals tracking)
    fn fn_of_prim(_p: egg::Symbol) -> fn(&[Val<Self>], &mut DomExpr<Self>) -> Val<Self> {
        unimplemented!()
    }
}

impl<D: Domain> DomExpr<D> {
    /// pretty prints the env->result pairs organized by node
    pub fn pretty_evals(&self) -> String {
        let mut s = format!("Evals for {}:",self);
        for id in 1..self.expr.nodes.len() {
            let id = Id::from(id);
            s.push_str(&format!(
                "\n\t{}:\n\t\t{}",
                self.expr.to_string_child(id),
                self.evals_of_node(id).iter().map(|(env,res)| format!("{:?} -> {:?}",env,res)).collect::<Vec<_>>().join("\n\t\t")
            ))
        }
        s
    }

    /// gets vec of (env,result) pairs for all the envs this node has been evaluated under
    pub fn evals_of_node(&self, node: Id) -> Vec<(Vec<Val<D>>,Val<D>)> {
        let flat: Vec<(&(Id,_),_)> = self.evals.iter().collect();
        flat.iter()
            .filter(|((id,_env),_res)| *id == node).map(|((_id,env),res)| (env.clone(),(*res).clone())).collect()
    }

    /// apply a function (Val) to an argument (Val)
    pub fn apply(&mut self, f: &Val<D>, x: &Val<D>) -> Val<D> {
        match f {
            Val::PrimFun(f) => f.apply(x, self),
            Val::LamClosure(f, env) => {
                let mut new_env = vec![x.clone()];
                new_env.extend(env.iter().cloned());
                self.eval_child(*f, &new_env)
            }
            _ => panic!("Expected function or closure"),
        }
    }
    /// eval the Expr in an environment
    pub fn eval(
        &mut self,
        env: &[Val<D>],
    ) -> Val<D> {
        self.eval_child(self.expr.root(), env)
    }

    /// eval a subexpression in an environment
    pub fn eval_child(
        &mut self,
        child: Id,
        env: &[Val<D>],
    ) -> Val<D> {
        let key = (child, env.to_vec());
        if let Some(val) = self.evals.get(&key).cloned() {
            return val;
        }
        let val = match self.expr.nodes[usize::from(child)] {
            Lambda::Var(i) => {
                env[i as usize].clone()
            }
            Lambda::IVar(_) => {
                panic!("attempting to execute a #i ivar")
            }
            Lambda::App([f,x]) => {
                let f_val = self.eval_child(f, env);
                let x_val = self.eval_child(x, env);
                self.apply(&f_val, &x_val)
            }
            Lambda::Prim(p) => {
                match D::val_of_prim(p) {
                    Some(v) => v.clone(),
                    None => panic!("Prim {} not found",p),
                }
            }
            Lambda::Lam([b]) => {
                Val::LamClosure(b, env.to_vec())
            }
            Lambda::Programs(_) => {
                panic!("not implemented")
            }
        };
        self.evals.insert(key, val.clone());
        val
    }
}