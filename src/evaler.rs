use crate::evalns::EvalNS;
use crate::error::Error;
use crate::grammar::{Expression, ExpressionTok::{EValue, EBinaryOp}, Value::{self, EConstant}, Constant, BinaryOp::{self, EPlus, EMinus, EMul, EDiv, EMod, EExp, ELT, ELTE, EEQ, ENE, EGTE, EGT, EOR, EAND}};
use crate::util::bool_to_f64;

use std::collections::HashSet;

//---- Types:

pub trait Evaler {
    fn eval(&self, ns:&mut EvalNS) -> Result<f64, Error>;

    fn var_names(&self) -> Result<HashSet<String>, Error> {
        let mut set = HashSet::new();
        {
            let mut ns = EvalNS::new(|name:&str| {
                set.insert(name.to_string());
                None
            });
            self.eval(&mut ns)?;
        }
        Ok(set)
    }
}

impl Evaler for Expression {
    fn eval(&self, ns:&mut EvalNS) -> Result<f64, Error> {
        if self.0.len()%2!=1 { return Err(Error::new("Expression len should always be odd")) }

        // Order of operations: 1) ^  2) */  3) +-
        // Exponentiation should be processed right-to-left.  Think of what 2^3^4 should mean:
        //     2^(3^4)=2417851639229258349412352   <--- I choose this one.
        //     (2^3)^4=4096
        // Direction of processing doesn't matter for Addition and Multiplication:
        //     (((3+4)+5)+6)==(3+(4+(5+6))), (((3*4)*5)*6)==(3*(4*(5*6)))
        // ...But Subtraction and Division must be processed left-to-right:
        //     (((6-5)-4)-3)!=(6-(5-(4-3))), (((6/5)/4)/3)!=(6/(5/(4/3)))


        // ---- Go code, for comparison ----
        // vals,ops:=make([]float64, len(e)/2+1),make([]BinaryOp, len(e)/2)
        // for i:=0; i<len(e); i+=2 {
        //     vals[i/2]=ns.EvalBubble(e[i].(evaler))
        //     if i<len(e)-1 { ops[i/2]=e[i+1].(BinaryOp) }
        // }

        let mut vals : Vec<f64>      = Vec::with_capacity(self.0.len()/2+1);
        let mut ops  : Vec<BinaryOp> = Vec::with_capacity(self.0.len()/2  );
        for (i,tok) in self.0.iter().enumerate() {
            eprintln!("expression tok: ({}, {:?})",i,tok);
            match tok {
                EValue(val) => {
                    if i%2==1 { return Err(Error::new("Found value at odd index")) }
                    match ns.eval_bubble(val) {
                        Ok(f) => vals.push(f),
                        Err(e) => return Err(e.pre(&format!("eval_bubble({:?})",val))),
                    }
                }
                EBinaryOp(bop) => {
                    if i%2==0 { return Err(Error::new("Found binaryop at even index")) }
                    ops.push(*bop);
                }
            }
        }


        // ---- Go code, for comparison ----
        // evalOp:=func(i int) {
        //     result:=ops[i]._Eval(vals[i], vals[i+1])
        //     vals=append(append(vals[:i], result), vals[i+2:]...)
        //     ops=append(ops[:i], ops[i+1:]...)
        // }
        // rtol:=func(s BinaryOp) { for i:=len(ops)-1; i>=0; i-- { if ops[i]==s { evalOp(i) } } }
        // ltor:=func(s BinaryOp) {
        //     loop:
        //     for i:=0; i<len(ops); i++ { if ops[i]==s { evalOp(i); goto loop } }  // Need to restart processing when modifying from the left.
        // }

        // I am defining rtol and ltor as 'fn' rather than closures to make it extra-clear that they don't capture anything.
        // I need to pass all those items around as args rather than just capturing because Rust doesn't like multiple closures to capture the same stuff when at least one of them mutates.
        let mut eval_op = |ops:&mut Vec<BinaryOp>, i:usize| {
            let result = ops[i].binaryop_eval(vals[i], vals[i+1]);
            vals[i]=result; vals.remove(i+1);
            ops.remove(i);
        };
        fn rtol(eval_op:&mut FnMut(&mut Vec<BinaryOp>,usize), ops:&mut Vec<BinaryOp>, op:BinaryOp) {
            // for-loop structure:
            let mut i = ops.len() as i64;
            loop { i-=1; if i<0 { break }
                let i = i as usize;

                if ops[i]==op { eval_op(ops,i); }
            }
        };
        fn ltor(eval_op:&mut FnMut(&mut Vec<BinaryOp>,usize), ops:&mut Vec<BinaryOp>, op:BinaryOp) {
            'outer: loop {
                // for-loop structure:
                let mut i : i64 = -1;
                loop { i+=1; if i>=ops.len() as i64 { break 'outer; }
                    let i = i as usize;

                    if ops[i]==op {
                        eval_op(ops,i);
                        continue 'outer;  // Need to restart processing when modifying from the left.
                    }
                }
            }
        };

        rtol(&mut eval_op, &mut ops, EExp);
        ltor(&mut eval_op, &mut ops, EMod);
        ltor(&mut eval_op, &mut ops, EDiv);
        rtol(&mut eval_op, &mut ops, EMul);
        ltor(&mut eval_op, &mut ops, EMinus);
        rtol(&mut eval_op, &mut ops, EPlus);
        ltor(&mut eval_op, &mut ops, ELT);
        ltor(&mut eval_op, &mut ops, EGT);
        ltor(&mut eval_op, &mut ops, ELTE);
        ltor(&mut eval_op, &mut ops, EGTE);
        ltor(&mut eval_op, &mut ops, EEQ);
        ltor(&mut eval_op, &mut ops, ENE);
        ltor(&mut eval_op, &mut ops, EAND);
        ltor(&mut eval_op, &mut ops, EOR);

        if ops.len()!=0 { return Err(Error::new("Unhandled Expression ops")); }
        if vals.len()!=1 { return Err(Error::new("More than one final Expression value")); }
        Ok(vals[0])
    }
}

impl Evaler for Value {
    fn eval(&self, ns:&mut EvalNS) -> Result<f64, Error> {
        match self {
            EConstant(c) => c.eval(ns),
        }
    }
}

impl Evaler for Constant {
    fn eval(&self, ns:&mut EvalNS) -> Result<f64, Error> { Ok(self.0) }
}

impl BinaryOp {
    // Non-standard eval interface (not generalized yet):
    fn binaryop_eval(&self, left:f64, right:f64) -> f64 {
        match self {
            EPlus => left+right,
            EMinus => left-right,
            EMul => left*right,
            EDiv => left/right,
            EMod => left%right, //left - (left/right).trunc()*right
            EExp => left.powf(right),
            ELT => bool_to_f64(left<right),
            ELTE => bool_to_f64(left<=right),
            EEQ => bool_to_f64(left==right),
            ENE => bool_to_f64(left!=right),
            EGTE => bool_to_f64(left>=right),
            EGT => bool_to_f64(left>right),
            EOR => if left!=0.0 { left }
                   else { right },
            EAND => if left==0.0 { left }
                    else { right },
        }
    }
}


//---- Tests:

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Parser;

    struct TestEvaler;
    impl Evaler for TestEvaler {
        fn eval(&self, ns:&mut EvalNS) -> Result<f64,Error> {
            match ns.get("x") {
                Some(v) => Ok(v),
                None => Ok(1.23),
            }
        }
    }

    #[test]
    fn var_names() {
        let p = Parser{
            is_const_byte:None,
            is_func_byte:None,
            is_var_byte:None,
        };
        assert_eq!(
            p.parse("12.34 + 43.21 + 11.11").unwrap().var_names().unwrap(),
            HashSet::new());

        let mut ns = EvalNS::new(|_| None);
        assert_eq!(
            p.parse("12.34 + 43.21 + 11.11").unwrap().eval(&mut ns),
            Ok(66.66));
    }
}

