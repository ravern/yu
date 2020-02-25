use thiserror::Error;

use crate::ast::{Atom, Expr, Function, List, Native, Operator};
use crate::env::Frame;

pub fn eval(expr: Expr) -> Result<Expr, EvalError> {
  let mut evalutor = Evaluator::new();
  evalutor.eval_expr(expr)
}

pub struct Evaluator {
  frame: Frame,
}

impl Evaluator {
  pub fn new() -> Evaluator {
    Evaluator {
      frame: Frame::new(),
    }
  }

  pub fn eval_expr(&mut self, expr: Expr) -> Result<Expr, EvalError> {
    use Expr::*;

    match expr {
      List(list) => self.eval_list(list),
      Atom(atom) => self.eval_atom(atom),
    }
  }

  pub fn eval_list(&mut self, list: List) -> Result<Expr, EvalError> {
    use Atom::*;
    use EvalError::*;
    use List::*;

    let node = match &list {
      Cons(node) => node.as_ref(),
      Nil => return Ok(Expr::List(Nil)),
    };

    let head = node.head.clone();
    let tail = node.tail.clone();

    let head = self.eval_expr(head)?;

    let function = match head {
      Expr::Atom(Function(function)) => function,
      Expr::Atom(Native(native)) => return self.eval_call_native(native, tail),
      _ => return Err(NotCallable),
    };

    if tail.len() != function.parameters().len() {
      return Err(WrongArity);
    }

    let original_frame = self.frame.clone();
    self.frame = Frame::with_parent(function.frame().clone());

    function
      .parameters()
      .into_iter()
      .zip(tail.into_iter())
      .map(|(symbol, expr)| {
        self.eval_call_define(List::cons(
          Expr::Atom(Symbol(symbol.clone())),
          List::cons(expr, Nil),
        ))
      })
      .collect::<Result<Vec<Expr>, EvalError>>()?;

    let expr = self.eval_expr(function.body().clone())?;

    self.frame = original_frame;

    Ok(expr)
  }

  pub fn eval_call_native(
    &mut self,
    native: Native,
    tail: List,
  ) -> Result<Expr, EvalError> {
    use Native::*;

    match native {
      Begin => self.eval_call_begin(tail),
      Define => self.eval_call_define(tail),
      Function => self.eval_call_function(tail),
      Quote => self.eval_call_quote(tail),
      Operator(operator) => self.eval_call_operator(operator, tail),
    }
  }

  pub fn eval_call_begin(&mut self, tail: List) -> Result<Expr, EvalError> {
    use EvalError::*;

    if tail.len() < 1 {
      return Err(WrongArity);
    }

    let mut tail = tail
      .into_iter()
      .map(|expr| self.eval_expr(expr))
      .collect::<Result<Vec<Expr>, EvalError>>()?;

    Ok(tail.pop().unwrap())
  }

  pub fn eval_call_define(&mut self, tail: List) -> Result<Expr, EvalError> {
    use EvalError::*;

    if tail.len() != 2 {
      return Err(WrongArity);
    }

    let symbol = self.as_symbol(tail.get(0).unwrap().clone())?;
    let expr = self.eval_expr(tail.get(1).unwrap().clone())?;

    self.frame.set(symbol, expr.clone());

    Ok(expr)
  }

  pub fn eval_call_function(&mut self, tail: List) -> Result<Expr, EvalError> {
    use EvalError::*;

    if tail.len() != 2 {
      return Err(WrongArity);
    }

    let parameters = self.as_list(tail.get(0).unwrap().clone())?;
    let body = tail.get(1).unwrap().clone();

    let parameters = parameters
      .into_iter()
      .map(|expr| self.as_symbol(expr))
      .collect::<Result<Vec<String>, EvalError>>()?;

    let frame = self.frame.clone();

    Ok(Expr::Atom(Atom::Function(Function::new(
      frame, parameters, body,
    ))))
  }

  pub fn eval_call_quote(&mut self, tail: List) -> Result<Expr, EvalError> {
    use EvalError::*;

    if tail.len() != 1 {
      return Err(WrongArity);
    }

    let expr = tail.get(0).unwrap().clone();

    Ok(expr)
  }

  pub fn eval_call_operator(
    &mut self,
    operator: Operator,
    tail: List,
  ) -> Result<Expr, EvalError> {
    use Operator::*;

    if tail.len() != 2 {
      return Ok(Expr::Atom(Atom::Number(0.0)));
    }

    let left = tail.get(0).unwrap().clone();
    let right = tail.get(1).unwrap().clone();

    let left = self.eval_expr(left)?;
    let right = self.eval_expr(right)?;

    let left = self.as_number(left)?;
    let right = self.as_number(right)?;

    let result = match operator {
      Add => left + right,
      Sub => left - right,
      Mul => left * right,
      Div => left / right,
    };

    Ok(Expr::Atom(Atom::Number(result)))
  }

  pub fn eval_atom(&mut self, atom: Atom) -> Result<Expr, EvalError> {
    use Atom::*;

    match atom {
      Symbol(symbol) => self.eval_symbol(symbol),
      atom => Ok(Expr::Atom(atom)),
    }
  }

  pub fn eval_symbol(&mut self, symbol: String) -> Result<Expr, EvalError> {
    use EvalError::*;

    if let Some(expr) = self.eval_special_symbol(&symbol) {
      return Ok(expr);
    }

    match self.frame.get(&symbol) {
      Some(expr) => Ok(expr.clone()),
      None => Err(UndefinedSymbol(symbol)),
    }
  }

  pub fn eval_special_symbol(&mut self, symbol: &str) -> Option<Expr> {
    use Native::{Begin, Define, Function, Quote};
    use Operator::*;

    let native = match symbol {
      "begin" => Begin,
      "define" => Define,
      "function" => Function,
      "quote" => Quote,
      "+" => Native::Operator(Add),
      "-" => Native::Operator(Sub),
      "*" => Native::Operator(Mul),
      "/" => Native::Operator(Div),
      _ => return None,
    };

    Some(Expr::Atom(Atom::Native(native)))
  }

  fn as_symbol(&mut self, expr: Expr) -> Result<String, EvalError> {
    use Atom::*;
    use EvalError::*;

    match expr {
      Expr::Atom(Symbol(symbol)) => Ok(symbol),
      _ => Err(InvalidType),
    }
  }

  fn as_list(&mut self, expr: Expr) -> Result<List, EvalError> {
    use EvalError::*;
    use Expr::*;

    match expr {
      List(list) => Ok(list),
      _ => Err(InvalidType),
    }
  }

  fn as_number(&mut self, expr: Expr) -> Result<f64, EvalError> {
    use Atom::*;
    use EvalError::*;

    match expr {
      Expr::Atom(Number(number)) => Ok(number),
      _ => Err(InvalidType),
    }
  }
}

#[derive(Debug, Error)]
pub enum EvalError {
  #[error("type is invalid")]
  InvalidType,
  #[error("arity is wrong")]
  WrongArity,
  #[error("'{0}' is undefined")]
  UndefinedSymbol(String),
  #[error("expression not callable")]
  NotCallable,
}
