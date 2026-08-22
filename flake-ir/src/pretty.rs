//! Textual dump of Flake IR.

use crate::ir::{Const, Function, Inst, Module};

pub fn print_module(module: &Module) -> String {
    let mut out = String::new();
    if !module.structs.is_empty() {
        for st in &module.structs {
            out.push_str(&format!("struct {} {{ ", st.name));
            for (i, (name, ty)) in st.fields.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&format!("{name}: {ty}"));
            }
            out.push_str(" }\n");
        }
        out.push('\n');
    }
    for (i, f) in module.functions.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        print_function(f, &mut out);
    }
    out
}

pub fn print_function(func: &Function, out: &mut String) {
    out.push_str("fn ");
    out.push_str(&func.name);
    out.push('(');
    for (i, pid) in func.params.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        let local = func.local(*pid).unwrap();
        let name = local.name.as_deref().unwrap_or("_");
        out.push_str(&format!("{name}: {}", local.ty));
    }
    out.push_str(&format!(") -> {}", func.ret));
    if func.effects_specified {
        out.push_str(" / ");
        if func.effects.is_empty() {
            out.push_str("pure");
        } else {
            out.push_str(&func.effects.join(" + "));
        }
    }
    if func.strict {
        out.push_str("  ; strict");
    }
    out.push_str(" {\n");
    for local in &func.locals {
        if func.params.contains(&local.id) {
            continue;
        }
        let name = local
            .name
            .clone()
            .unwrap_or_else(|| format!("t{}", local.id.0));
        out.push_str(&format!(
            "    local %{} {} : {}\n",
            local.id.0, name, local.ty
        ));
    }
    for block in &func.blocks {
        out.push_str(&format!("  bb{}:\n", block.id.0));
        for inst in &block.insts {
            out.push_str("    ");
            out.push_str(&format_inst(inst));
            out.push('\n');
        }
    }
    out.push_str("}\n");
}

fn format_inst(inst: &Inst) -> String {
    match inst {
        Inst::LoadConst { dest, value } => format!("%{} = const {}", dest.0, format_const(value)),
        Inst::LoadFunction { dest, name } => format!("%{} = fnaddr {name}", dest.0),
        Inst::Move { dest, src } => format!("%{} = %{}", dest.0, src.0),
        Inst::Binary {
            dest, op, lhs, rhs, ..
        } => format!("%{} = {} %{}, %{}", dest.0, op.as_str(), lhs.0, rhs.0),
        Inst::Unary { dest, op, src } => format!("%{} = {} %{}", dest.0, op.as_str(), src.0),
        Inst::Call { dest, callee, args } => {
            let args = args
                .iter()
                .map(|a| format!("%{}", a.0))
                .collect::<Vec<_>>()
                .join(", ");
            let call = match callee {
                crate::ir::Callee::Static(n) => format!("call {n}({args})"),
                crate::ir::Callee::Local(id) => format!("call %{}({args})", id.0),
            };
            match dest {
                Some(d) => format!("%{} = {call}", d.0),
                None => call,
            }
        }
        Inst::GetIndex { dest, obj, index } => {
            format!("%{} = %{}[%{}]", dest.0, obj.0, index.0)
        }
        Inst::SetIndex { obj, index, value } => {
            format!("%{}[%{}] = %{}", obj.0, index.0, value.0)
        }
        Inst::GetField { dest, obj, field } => format!("%{} = %{}.{}", dest.0, obj.0, field),
        Inst::SetField { obj, field, value } => format!("%{}.{} = %{}", obj.0, field, value.0),
        Inst::MakeList { dest, items } => {
            let items = items
                .iter()
                .map(|i| format!("%{}", i.0))
                .collect::<Vec<_>>()
                .join(", ");
            format!("%{} = list [{items}]", dest.0)
        }
        Inst::MakeMap { dest, keys, values } => {
            let pairs: Vec<_> = keys
                .iter()
                .zip(values)
                .map(|(k, v)| format!("%{}: %{}", k.0, v.0))
                .collect();
            format!("%{} = map {{{}}}", dest.0, pairs.join(", "))
        }
        Inst::MakeStruct { dest, name, fields } => {
            let fields: Vec<_> = fields
                .iter()
                .map(|(n, v)| format!("{n}: %{}", v.0))
                .collect();
            format!("%{} = struct {name} {{ {} }}", dest.0, fields.join(", "))
        }
        Inst::MakeRange { dest, start, end } => {
            format!("%{} = range %{} .. %{}", dest.0, start.0, end.0)
        }
        Inst::MakeIter { dest, src } => format!("%{} = iter %{}", dest.0, src.0),
        Inst::IterNext { value, more, iter } => {
            format!("%{}, %{} = iternext %{}", value.0, more.0, iter.0)
        }
        Inst::Concat { dest, parts } => {
            let parts = parts
                .iter()
                .map(|p| format!("%{}", p.0))
                .collect::<Vec<_>>()
                .join(", ");
            format!("%{} = concat {parts}", dest.0)
        }
        Inst::Jump { target } => format!("goto bb{}", target.0),
        Inst::Branch {
            cond,
            then_block,
            else_block,
        } => format!("br %{} bb{}, bb{}", cond.0, then_block.0, else_block.0),
        Inst::Return { value } => match value {
            Some(v) => format!("return %{}", v.0),
            None => "return".into(),
        },
    }
}

fn format_const(c: &Const) -> String {
    match c {
        Const::Nil => "nil".into(),
        Const::Bool(b) => b.to_string(),
        Const::Int(n) => n.to_string(),
        Const::Float(n) => n.to_string(),
        Const::String(s) => format!("{s:?}"),
    }
}
