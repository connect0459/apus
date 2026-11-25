use crate::ast::{BinOp, UnaryOp};
use crate::hash::Fnv1a;
use crate::tir::{LocalId, TExpr, TFunc, TStmt, TStruct};
use crate::types::Ty;

pub fn function_key(func: &TFunc, structs: &[TStruct], compiler_stamp: u64) -> u64 {
    let mut encoder = Encoder {
        hasher: Fnv1a::new(),
        structs,
    };
    encoder.u64(compiler_stamp);
    encoder.str(func.name());
    encoder.usize(func.param_types().len());
    encoder.ty(func.ret());
    encoder.usize(func.locals().len());
    for ty in func.locals() {
        encoder.ty(*ty);
    }
    encoder.stmts(func.body());
    encoder.hasher.finish()
}

struct Encoder<'a> {
    hasher: Fnv1a,
    structs: &'a [TStruct],
}

impl Encoder<'_> {
    fn tag(&mut self, tag: u8) {
        self.hasher.write(&[tag]);
    }

    fn u64(&mut self, value: u64) {
        self.hasher.write(&value.to_le_bytes());
    }

    fn usize(&mut self, value: usize) {
        self.u64(value as u64);
    }

    fn str(&mut self, value: &str) {
        self.usize(value.len());
        self.hasher.write(value.as_bytes());
    }

    fn ty(&mut self, ty: Ty) {
        match ty {
            Ty::Int => self.tag(1),
            Ty::Bool => self.tag(2),
            Ty::Str => self.tag(3),
            Ty::Void => self.tag(4),
            Ty::Enum(_) => self.tag(6),
            Ty::Param(index) => {
                self.tag(7);
                self.usize(index);
            }
            Ty::Assoc(packed) => {
                let (index, id) = Ty::split_assoc(packed);
                self.tag(9);
                self.usize(index);
                self.usize(id);
            }
            Ty::Struct(id) | Ty::Class(id) => {
                self.tag(if ty.is_class() { 8 } else { 5 });
                let layout = self.structs.get(id.index());
                let fields = layout.map_or(&[][..], TStruct::fields);
                self.usize(fields.len());
                for field in fields {
                    if field.is_class() {
                        self.tag(8);
                    } else {
                        self.ty(*field);
                    }
                }
                let starts = layout.map_or(&[][..], TStruct::starts);
                self.usize(starts.len());
                for start in starts {
                    self.usize(*start);
                }
            }
        }
    }

    fn local(&mut self, local: LocalId) {
        self.usize(local.index());
    }

    fn stmts(&mut self, stmts: &[TStmt]) {
        self.usize(stmts.len());
        for stmt in stmts {
            self.stmt(stmt);
        }
    }

    fn stmt(&mut self, stmt: &TStmt) {
        match stmt {
            TStmt::Let { local, init } => {
                self.tag(10);
                self.local(*local);
                self.expr(init);
            }
            TStmt::Assign { local, value } => {
                self.tag(11);
                self.local(*local);
                self.expr(value);
            }
            TStmt::AssignField {
                local,
                index,
                value,
            } => {
                self.tag(12);
                self.local(*local);
                self.usize(*index);
                self.expr(value);
            }
            TStmt::Return(value) => {
                self.tag(13);
                self.optional_expr(value.as_ref());
            }
            TStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                self.tag(14);
                self.expr(cond);
                self.stmts(then_body);
                self.stmts(else_body);
            }
            TStmt::Break(depth) => {
                self.tag(23);
                self.usize(*depth);
            }
            TStmt::Continue(depth) => {
                self.tag(24);
                self.usize(*depth);
            }
            TStmt::Block(body) => {
                self.tag(25);
                self.stmts(body);
            }
            TStmt::While { cond, body } => {
                self.tag(15);
                self.expr(cond);
                self.stmts(body);
            }
            TStmt::ForRange {
                local,
                lo,
                hi,
                inclusive,
                body,
            } => {
                self.tag(16);
                self.local(*local);
                self.expr(lo);
                self.expr(hi);
                self.tag(u8::from(*inclusive));
                self.stmts(body);
            }
            TStmt::Print(expr) => {
                self.tag(17);
                self.expr(expr);
            }
            TStmt::Eval(expr) => {
                self.tag(18);
                self.expr(expr);
            }
            TStmt::Release(expr) => {
                self.tag(19);
                self.expr(expr);
            }
            TStmt::Clear(local) => {
                self.tag(20);
                self.local(*local);
            }
            TStmt::Free(expr) => {
                self.tag(21);
                self.expr(expr);
            }
            TStmt::ReleaseUnowned(expr) => {
                self.tag(22);
                self.expr(expr);
            }
        }
    }

    fn optional_expr(&mut self, expr: Option<&TExpr>) {
        match expr {
            Some(expr) => {
                self.tag(1);
                self.expr(expr);
            }
            None => self.tag(0),
        }
    }

    fn expr(&mut self, expr: &TExpr) {
        match expr {
            TExpr::Int(n) => {
                self.tag(30);
                self.hasher.write(&n.to_le_bytes());
            }
            TExpr::Bool(b) => {
                self.tag(31);
                self.tag(u8::from(*b));
            }
            TExpr::Str(text) => {
                self.tag(32);
                self.str(text);
            }
            TExpr::Local(local, ty) => {
                self.tag(33);
                self.local(*local);
                self.ty(*ty);
            }
            TExpr::Unary { op, expr } => {
                self.tag(34);
                self.tag(match op {
                    UnaryOp::Neg => 0,
                    UnaryOp::Not => 1,
                });
                self.expr(expr);
            }
            TExpr::Binary { op, lhs, rhs } => {
                self.tag(35);
                self.tag(bin_op_code(*op));
                self.expr(lhs);
                self.expr(rhs);
            }
            TExpr::Call {
                callee,
                type_args,
                args,
                ret,
            } => {
                self.tag(36);
                self.str(callee);
                self.usize(type_args.len());
                for ty in type_args {
                    self.ty(*ty);
                }
                self.ty(*ret);
                self.usize(args.len());
                for arg in args {
                    self.expr(arg);
                }
            }
            TExpr::Dispatch {
                protocol,
                method,
                args,
                ret,
            } => {
                self.tag(54);
                self.str(protocol);
                self.str(method);
                self.ty(*ret);
                self.usize(args.len());
                for arg in args {
                    self.expr(arg);
                }
            }
            TExpr::Witness {
                protocol,
                method,
                self_ty,
                args,
                ret,
            } => {
                self.tag(53);
                self.str(protocol);
                self.str(method);
                self.ty(*self_ty);
                self.ty(*ret);
                self.usize(args.len());
                for arg in args {
                    self.expr(arg);
                }
            }
            TExpr::Virtual {
                cases,
                default,
                args,
                ret,
            } => {
                self.tag(42);
                self.usize(cases.len());
                for (tag, callee) in cases {
                    self.hasher.write(&tag.to_le_bytes());
                    self.str(callee);
                }
                self.str(default);
                self.ty(*ret);
                self.usize(args.len());
                for arg in args {
                    self.expr(arg);
                }
            }
            TExpr::StructInit { ty, args } => {
                self.tag(37);
                self.ty(*ty);
                self.usize(args.len());
                for arg in args {
                    self.expr(arg);
                }
            }
            TExpr::New { ty, args } => {
                self.tag(40);
                self.ty(*ty);
                self.usize(args.len());
                for arg in args {
                    self.expr(arg);
                }
            }
            TExpr::Field { base, index, ty } => {
                self.tag(38);
                self.expr(base);
                self.usize(*index);
                self.ty(*ty);
            }
            TExpr::EnumCase { ty, index } => {
                self.tag(39);
                self.ty(*ty);
                self.usize(*index);
            }
            TExpr::Zero(ty) => {
                self.tag(41);
                self.ty(*ty);
            }
            TExpr::Retain(inner) => {
                self.tag(43);
                self.expr(inner);
            }
            TExpr::Moved(inner) => {
                self.tag(44);
                self.expr(inner);
            }
            TExpr::Bind { local, init } => {
                self.tag(45);
                self.local(*local);
                self.expr(init);
            }
            TExpr::FuncAddr(name) => {
                self.tag(46);
                self.str(name);
            }
            TExpr::Select {
                cond,
                then,
                otherwise,
                ty,
            } => {
                self.tag(50);
                self.expr(cond);
                self.expr(then);
                self.expr(otherwise);
                self.ty(*ty);
            }
            TExpr::Unwrap { base, ty } => {
                self.tag(49);
                self.expr(base);
                self.ty(*ty);
            }
            TExpr::RetainUnowned(inner) => {
                self.tag(47);
                self.expr(inner);
            }
            TExpr::LoadUnowned(inner) => {
                self.tag(48);
                self.expr(inner);
            }
            TExpr::RetainWeak(inner) => {
                self.tag(51);
                self.expr(inner);
            }
            TExpr::LoadWeak(inner) => {
                self.tag(52);
                self.expr(inner);
            }
        }
    }
}

fn bin_op_code(op: BinOp) -> u8 {
    match op {
        BinOp::Add => 0,
        BinOp::Sub => 1,
        BinOp::Mul => 2,
        BinOp::Div => 3,
        BinOp::Rem => 4,
        BinOp::Eq => 5,
        BinOp::Ne => 6,
        BinOp::Lt => 7,
        BinOp::Gt => 8,
        BinOp::Le => 9,
        BinOp::Ge => 10,
        BinOp::And => 11,
        BinOp::Or => 12,
        BinOp::Is => 13,
        BinOp::IsNot => 14,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Decl;
    use crate::check::{Signatures, check_func};
    use crate::items::split_items;
    use crate::lexer::lex;
    use crate::parser::parse_item;

    const STAMP: u64 = 7;

    fn keys_with_stamp(src: &str, stamp: u64) -> Vec<(String, u64)> {
        let decls: Vec<Decl> = split_items(&lex(src).unwrap())
            .unwrap()
            .iter()
            .map(|item| parse_item(item.tokens()).unwrap())
            .collect();
        let sigs = Signatures::collect(&decls).unwrap();
        let structs = sigs.layouts();
        decls
            .iter()
            .filter_map(|d| match d {
                Decl::Func(f) => Some(f),
                Decl::Struct(_) | Decl::Enum(_) | Decl::Protocol(_) => None,
            })
            .map(|f| {
                let checked = check_func(f, &sigs).unwrap();
                (
                    f.name().to_string(),
                    function_key(&checked, &structs, stamp),
                )
            })
            .collect()
    }

    fn keys(src: &str) -> Vec<(String, u64)> {
        keys_with_stamp(src, STAMP)
    }

    fn key_of(src: &str, name: &str) -> u64 {
        keys(src).into_iter().find(|(n, _)| n == name).unwrap().1
    }

    #[test]
    fn identical_source_gives_identical_keys() {
        let src = "func f() -> Int { return 1 }\nfunc main() { print(f()) }";
        assert_eq!(keys(src), keys(src));
    }

    #[test]
    fn whitespace_and_comments_do_not_change_keys() {
        let plain = "func f() -> Int { return 1 }";
        let noisy = "// note\nfunc  f( )  ->  Int {\n\n  return 1 /* c */\n}";
        assert_eq!(keys(plain), keys(noisy));
    }

    #[test]
    fn editing_a_function_changes_only_its_own_key() {
        let before = "func a() -> Int { return 1 }\nfunc b() -> Int { return 2 }";
        let after = "func a() -> Int { return 1 }\nfunc b() -> Int { return 3 }";
        assert_eq!(key_of(before, "a"), key_of(after, "a"));
        assert_ne!(key_of(before, "b"), key_of(after, "b"));
    }

    #[test]
    fn editing_a_callees_body_does_not_change_its_callers_key() {
        let before = "func g() -> Int { return 1 }\nfunc f() -> Int { return g() }";
        let after = "func g() -> Int { return 2 }\nfunc f() -> Int { return g() }";
        assert_eq!(key_of(before, "f"), key_of(after, "f"));
        assert_ne!(key_of(before, "g"), key_of(after, "g"));
    }

    #[test]
    fn changing_a_callees_result_type_changes_its_callers_key() {
        let before = "func g() -> Int { return 1 }\nfunc f() { print(g()) }";
        let after = "func g() -> Bool { return true }\nfunc f() { print(g()) }";
        assert_ne!(key_of(before, "f"), key_of(after, "f"));
    }

    #[test]
    fn adding_or_reordering_functions_does_not_change_other_keys() {
        let base = "func a() -> Int { return 1 }\nfunc b() -> Int { return 2 }";
        let extended =
            "func extra() { }\nfunc b() -> Int { return 2 }\nfunc a() -> Int { return 1 }";
        assert_eq!(key_of(base, "a"), key_of(extended, "a"));
        assert_eq!(key_of(base, "b"), key_of(extended, "b"));
    }

    #[test]
    fn a_function_over_a_class_and_one_over_a_struct_with_the_same_members_have_different_keys() {
        let body = "func f(_ v: T) -> Int { return v.x }\n";
        let as_struct = key_of(&format!("struct T {{ var x: Int }}\n{body}"), "f");
        let as_class = key_of(&format!("class T {{ var x: Int }}\n{body}"), "f");
        assert_ne!(as_struct, as_class);
    }

    #[test]
    fn allocating_a_class_instance_differs_from_building_a_struct_value() {
        let make = "func f() -> Int { let v = T(x: 1)\n return v.x }\n";
        let as_struct = key_of(&format!("struct T {{ var x: Int }}\n{make}"), "f");
        let as_class = key_of(&format!("class T {{ var x: Int }}\n{make}"), "f");
        assert_ne!(as_struct, as_class);
    }

    #[test]
    fn a_different_compiler_stamp_gives_different_keys() {
        let src = "func f() { }";
        assert_ne!(keys_with_stamp(src, 1), keys_with_stamp(src, 2));
    }

    const TWO_STRUCTS: &str = "struct A { var x: Int }\nstruct B { var y: Int }\n\
        func useA() -> Int { let a = A(x: 1)\nreturn a.x }\n\
        func useB() -> Int { let b = B(y: 1)\nreturn b.y }\n";

    #[test]
    fn reordering_struct_declarations_does_not_change_keys() {
        let swapped = "struct B { var y: Int }\nstruct A { var x: Int }\n\
            func useA() -> Int { let a = A(x: 1)\nreturn a.x }\n\
            func useB() -> Int { let b = B(y: 1)\nreturn b.y }\n";
        assert_eq!(key_of(TWO_STRUCTS, "useA"), key_of(swapped, "useA"));
        assert_eq!(key_of(TWO_STRUCTS, "useB"), key_of(swapped, "useB"));
    }

    #[test]
    fn changing_a_struct_layout_changes_only_the_functions_that_use_it() {
        let widened = TWO_STRUCTS
            .replace(
                "struct A { var x: Int }",
                "struct A { var x: Int; var z: Int }",
            )
            .replace("A(x: 1)", "A(x: 1, z: 2)");
        assert_ne!(key_of(TWO_STRUCTS, "useA"), key_of(&widened, "useA"));
        assert_eq!(key_of(TWO_STRUCTS, "useB"), key_of(&widened, "useB"));
    }

    #[test]
    fn the_key_of_a_known_function_is_stable_across_runs_and_toolchains() {
        assert_eq!(key_of("func f() { print(1) }", "f"), GOLDEN_KEY);
    }

    const GOLDEN_KEY: u64 = 17498113945502841606;

    const SWITCH: &str =
        "func pick(_ c: Color) -> Int {\nswitch c {\ncase .red: return 1\ndefault: return 2\n}\n}";

    #[test]
    fn renaming_an_enum_does_not_change_the_key_of_a_function_using_it() {
        let a = format!("enum Color {{ case red, green }}\n{SWITCH}");
        let b = format!(
            "enum Hue {{ case red, green }}\n{}",
            SWITCH.replace("Color", "Hue")
        );
        assert_eq!(key_of(&a, "pick"), key_of(&b, "pick"));
    }

    #[test]
    fn selecting_a_different_enum_case_changes_the_key() {
        let a = format!("enum Color {{ case red, green }}\n{SWITCH}");
        let b = a.replace("case .red", "case .green");
        assert_ne!(key_of(&a, "pick"), key_of(&b, "pick"));
    }
}
