extern crate proc_macro;
use proc_macro::TokenStream;
use proc_macro2::Span;

use luaparse::ast::{ForStat, FunctionBody, FunctionDeclarationStat, Name, Statement, Var};
use luaparse::error::Error as LError;
use luaparse::HasSpan;
use quote::quote;
use rlua::Lua;
use syn::Error as SynError;

#[proc_macro]
pub fn define(input: TokenStream) -> TokenStream {
    let s = input.to_string();
    let mut lua_err = None;
    Lua::new().context(|lua| {
        let chunk = lua.load(&s);
        let r = chunk.exec();
        if let Err(err) = r {
            lua_err = Some(err.to_string());
        }
    });

    let errors = validate_aerospike(&s);
    let mut syn_errs = vec![];

    for e in errors {
        syn_errs.push(SynError::new(Span::call_site(), &e));
    }

    if let Some(f) = syn_errs.first() {
        let mut f_err = f.clone();

        if syn_errs.len() > 1 {
            for e in &syn_errs[1..] {
                f_err.combine(e.clone());
            }
        }
        return f_err.into_compile_error().into();
    }

    if let Some(err) = lua_err {
        return SynError::new(Span::call_site(), err)
            .into_compile_error()
            .into();
    }

    let tokens = quote! {#s};

    tokens.into()
}

fn validate_aerospike(s: &str) -> Vec<String> {
    let mut errs = vec![];
    match luaparse::parse(s) {
        Ok(block) => {
            loop_statements(&block.statements, &mut errs, true);
        }
        Err(e) => panic!("{:#}", LError::new(e.span(), e).with_buffer(s)),
    }
    errs
}

const AEROSPIKE_NAMES: [&str; 9] = [
    "record",
    "map",
    "list",
    "aerospike",
    "bytes",
    "geojson",
    "iterator",
    "list",
    "stream",
];

fn is_reserved(s: &str) -> bool {
    for aerospike_name in AEROSPIKE_NAMES {
        if aerospike_name == s {
            return true;
        }
    }
    false
}

fn validate_func(body: &FunctionBody, errors: &mut Vec<String>, is_global: bool) {
    let names = body.params.list.pairs.iter().map(|a| a.0.clone()).collect();
    validate_names(&names, errors, true);
    loop_statements(&body.block.statements, errors, is_global);
}

fn loop_statements(stmts: &Vec<Statement>, errors: &mut Vec<String>, is_global: bool) {
    for statement in stmts {
        recurse(statement, errors, is_global);
    }
}

fn validate_names(names: &Vec<Name>, errors: &mut Vec<String>, allow_vars: bool) {
    for param in names {
        let name = param.to_string();
        if is_reserved(&name) {
            errors.push(format!(
                "aerospike reserved identifier: `{}`. consider renaming your variable",
                name
            ));
        }
        if !allow_vars {
            errors.push(format!("global variables are not allowed: `{}`", name));
        }
    }
}

fn recurse(stmt: &Statement, errors: &mut Vec<String>, mut is_global: bool) {
    let mut allow_vars = true;
    if is_global {
        is_global = false;
        allow_vars = false;
    }
    match stmt {
        Statement::FunctionDeclaration(func) => match func {
            FunctionDeclarationStat::Local { body, .. } => {
                validate_func(&body, errors, is_global);
            }
            FunctionDeclarationStat::Nonlocal { body, .. } => {
                validate_func(&body, errors, is_global);
            }
        },
        Statement::LocalDeclaration(ld) => {
            let names = ld.names.pairs.iter().map(|a| a.0.clone()).collect();
            validate_names(&names, errors, allow_vars);
        }
        Statement::Assignment(ass) => {
            let names = ass
                .vars
                .pairs
                .iter()
                .filter(|a| {
                    if let Var::Name(_n) = &a.0 {
                        return true;
                    }
                    return false;
                })
                .map(|a| {
                    if let Var::Name(n) = &a.0 {
                        return n.clone();
                    }
                    panic!("impossible")
                })
                .collect();

            validate_names(&names, errors, allow_vars);
        }
        Statement::If(i) => {
            loop_statements(&i.block.statements, errors, is_global);
            if let Some(el) = &i.else_ {
                loop_statements(&el.block.statements, errors, is_global);
            }

            for elseif in &i.elseifs {
                loop_statements(&elseif.block.statements, errors, is_global);
            }
        }
        Statement::While(wl) => {
            loop_statements(&wl.block.statements, errors, is_global);
        }
        Statement::For(f) => match f {
            ForStat::Generic(fg) => {
                loop_statements(&fg.block.statements, errors, is_global);
            }
            ForStat::Numerical(n) => {
                loop_statements(&n.block.statements, errors, is_global);
            }
        },
        Statement::Repeat(rp) => {
            loop_statements(&rp.block.statements, errors, is_global);
        }
        _ => {}
    }
}
