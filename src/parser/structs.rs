fn __parse_struct_or_union_specifier<'input>(__input: &'input str, __state: &mut ParseState<'input>, __pos: usize, env: &mut Env) -> RuleResult<StructType> {
    #![allow(non_snake_case, unused)]
    {
        let __choice_res = {
            let __seq_res = {
                let __seq_res = Matched(__pos, __pos);
                match __seq_res {
                    Matched(__pos, l) => {
                        let __seq_res = __parse_struct_or_union(__input, __state, __pos, env);
                        match __seq_res {
                            Matched(__pos, e) => {
                                let __seq_res = Matched(__pos, __pos);
                                match __seq_res {
                                    Matched(__pos, r) => Matched(__pos, { Node::new(e, Span::span(l, r)) }),
                                    Failed => Failed,
                                }
                            }
                            Failed => Failed,
                        }
                    }
                    Failed => Failed,
                }
            };
            match __seq_res {
                Matched(__pos, t) => {
                    let __seq_res = __parse__(__input, __state, __pos, env);
                    match __seq_res {
                        Matched(__pos, _) => {
                            // GNU attributes between the keyword and the tag:
                            // `struct __attribute__((aligned(64))) name { ... }`.
                            let (__pos, x) = match __parse_attribute_specifier_list(__input, __state, __pos, env) {
                                Matched(__attr_pos, x) if !x.is_empty() => match __parse__(__input, __state, __attr_pos, env) {
                                    Matched(__after, _) => (__after, x),
                                    Failed => (__attr_pos, x),
                                },
                                _ => (__pos, Vec::new()),
                            };
                            let __seq_res = match __parse_identifier(__input, __state, __pos, env) {
                                Matched(__newpos, __value) => Matched(__newpos, Some(__value)),
                                Failed => Matched(__pos, None),
                            };
                            match __seq_res {
                                Matched(__pos, i) => {
                                    let __seq_res = __parse__(__input, __state, __pos, env);
                                    match __seq_res {
                                        Matched(__pos, _) => {
                                            let __seq_res = __parse_struct_or_union_body(__input, __state, __pos, env);
                                            match __seq_res {
                                                Matched(__pos, d) => Matched(__pos, { StructType { kind: t, identifier: i, declarations: d, extensions: x } }),
                                                Failed => Failed,
                                            }
                                        }
                                        Failed => Failed,
                                    }
                                }
                                Failed => Failed,
                            }
                        }
                        Failed => Failed,
                    }
                }
                Failed => Failed,
            }
        };
        match __choice_res {
            Matched(__pos, __value) => Matched(__pos, __value),
            Failed => {
                let __seq_res = {
                    let __seq_res = Matched(__pos, __pos);
                    match __seq_res {
                        Matched(__pos, l) => {
                            let __seq_res = __parse_struct_or_union(__input, __state, __pos, env);
                            match __seq_res {
                                Matched(__pos, e) => {
                                    let __seq_res = Matched(__pos, __pos);
                                    match __seq_res {
                                        Matched(__pos, r) => Matched(__pos, { Node::new(e, Span::span(l, r)) }),
                                        Failed => Failed,
                                    }
                                }
                                Failed => Failed,
                            }
                        }
                        Failed => Failed,
                    }
                };
                match __seq_res {
                    Matched(__pos, t) => {
                        let __seq_res = __parse__(__input, __state, __pos, env);
                        match __seq_res {
                            Matched(__pos, _) => {
                                let __seq_res = __parse_identifier(__input, __state, __pos, env);
                                match __seq_res {
                                    Matched(__pos, i) => Matched(__pos, { StructType { kind: t, identifier: Some(i), declarations: None, extensions: Vec::new() } }),
                                    Failed => Failed,
                                }
                            }
                            Failed => Failed,
                        }
                    }
                    Failed => Failed,
                }
            }
        }
    }
}

fn __parse_struct_or_union_body<'input>(__input: &'input str, __state: &mut ParseState<'input>, __pos: usize, env: &mut Env) -> RuleResult<Option<Vec<Node<StructDeclaration>>>> {
    #![allow(non_snake_case, unused)]
    {
        let __choice_res = {
            let __seq_res = slice_eq(__input, __state, __pos, "{");
            match __seq_res {
                Matched(__pos, _) => {
                    let __seq_res = __parse__(__input, __state, __pos, env);
                    match __seq_res {
                        Matched(__pos, _) => {
                            let __seq_res = {
                                let __seq_res = {
                                    let mut __repeat_pos = __pos;
                                    let mut __repeat_value = vec![];
                                    loop {
                                        let __pos = __repeat_pos;
                                        let __pos = if __repeat_value.len() > 0 {
                                            let __sep_res = __parse__(__input, __state, __pos, env);
                                            match __sep_res {
                                                Matched(__newpos, _) => __newpos,
                                                Failed => break,
                                            }
                                        } else {
                                            __pos
                                        };
                                        let __step_res = {
                                            let __seq_res = Matched(__pos, __pos);
                                            match __seq_res {
                                                Matched(__pos, l) => {
                                                    let __seq_res = __parse_struct_declaration(__input, __state, __pos, env);
                                                    match __seq_res {
                                                        Matched(__pos, e) => {
                                                            let __seq_res = Matched(__pos, __pos);
                                                            match __seq_res {
                                                                Matched(__pos, r) => Matched(__pos, { Node::new(e, Span::span(l, r)) }),
                                                                Failed => Failed,
                                                            }
                                                        }
                                                        Failed => Failed,
                                                    }
                                                }
                                                Failed => Failed,
                                            }
                                        };
                                        match __step_res {
                                            Matched(__newpos, __value) => {
                                                __repeat_pos = __newpos;
                                                __repeat_value.push(__value);
                                            }
                                            Failed => {
                                                break;
                                            }
                                        }
                                    }
                                    if __repeat_value.len() >= 1 {
                                        Matched(__repeat_pos, __repeat_value)
                                    } else {
                                        Failed
                                    }
                                };
                                match __seq_res {
                                    Matched(__pos, e) => Matched(__pos, { e }),
                                    Failed => Failed,
                                }
                            };
                            match __seq_res {
                                Matched(__pos, d) => {
                                    let __seq_res = __parse__(__input, __state, __pos, env);
                                    match __seq_res {
                                        Matched(__pos, _) => {
                                            let __seq_res = slice_eq(__input, __state, __pos, "}");
                                            match __seq_res {
                                                Matched(__pos, _) => Matched(__pos, { Some(d) }),
                                                Failed => Failed,
                                            }
                                        }
                                        Failed => Failed,
                                    }
                                }
                                Failed => Failed,
                            }
                        }
                        Failed => Failed,
                    }
                }
                Failed => Failed,
            }
        };
        match __choice_res {
            Matched(__pos, __value) => Matched(__pos, __value),
            Failed => {
                let __choice_res = {
                    let __seq_res = {
                        let __seq_res = {
                            __state.suppress_fail += 1;
                            let __assert_res = __parse_gnu_guard(__input, __state, __pos, env);
                            __state.suppress_fail -= 1;
                            match __assert_res {
                                Matched(_, __value) => Matched(__pos, __value),
                                Failed => Failed,
                            }
                        };
                        match __seq_res {
                            Matched(__pos, _) => {
                                let __seq_res = {
                                    let __seq_res = slice_eq(__input, __state, __pos, "{");
                                    match __seq_res {
                                        Matched(__pos, _) => {
                                            let __seq_res = __parse__(__input, __state, __pos, env);
                                            match __seq_res {
                                                Matched(__pos, _) => slice_eq(__input, __state, __pos, "}"),
                                                Failed => Failed,
                                            }
                                        }
                                        Failed => Failed,
                                    }
                                };
                                match __seq_res {
                                    Matched(__pos, e) => Matched(__pos, { e }),
                                    Failed => Failed,
                                }
                            }
                            Failed => Failed,
                        }
                    };
                    match __seq_res {
                        Matched(__pos, _) => Matched(__pos, { Some(Vec::new()) }),
                        Failed => Failed,
                    }
                };
                match __choice_res {
                    Matched(__pos, __value) => Matched(__pos, __value),
                    Failed => Matched(__pos, { None }),
                }
            }
        }
    }
}

fn __parse_struct_or_union<'input>(__input: &'input str, __state: &mut ParseState<'input>, __pos: usize, env: &mut Env) -> RuleResult<StructKind> {
    #![allow(non_snake_case, unused)]
    {
        let __choice_res = {
            let __seq_res = {
                __state.suppress_fail += 1;
                let res = {
                    let __seq_res = slice_eq(__input, __state, __pos, "struct");
                    match __seq_res {
                        Matched(__pos, e) => {
                            let __seq_res = {
                                __state.suppress_fail += 1;
                                let __assert_res = if __input.len() > __pos {
                                    let (__ch, __next) = char_range_at(__input, __pos);
                                    match __ch {
                                        '_' | 'a'..='z' | 'A'..='Z' | '0'..='9' => Matched(__next, ()),
                                        _ => __state.mark_failure(__pos, "[_a-zA-Z0-9]"),
                                    }
                                } else {
                                    __state.mark_failure(__pos, "[_a-zA-Z0-9]")
                                };
                                __state.suppress_fail -= 1;
                                match __assert_res {
                                    Failed => Matched(__pos, ()),
                                    Matched(..) => Failed,
                                }
                            };
                            match __seq_res {
                                Matched(__pos, _) => Matched(__pos, { e }),
                                Failed => Failed,
                            }
                        }
                        Failed => Failed,
                    }
                };
                __state.suppress_fail -= 1;
                res
            };
            match __seq_res {
                Matched(__pos, _) => Matched(__pos, { StructKind::Struct }),
                Failed => Failed,
            }
        };
        match __choice_res {
            Matched(__pos, __value) => Matched(__pos, __value),
            Failed => {
                let __seq_res = {
                    __state.suppress_fail += 1;
                    let res = {
                        let __seq_res = slice_eq(__input, __state, __pos, "union");
                        match __seq_res {
                            Matched(__pos, e) => {
                                let __seq_res = {
                                    __state.suppress_fail += 1;
                                    let __assert_res = if __input.len() > __pos {
                                        let (__ch, __next) = char_range_at(__input, __pos);
                                        match __ch {
                                            '_' | 'a'..='z' | 'A'..='Z' | '0'..='9' => Matched(__next, ()),
                                            _ => __state.mark_failure(__pos, "[_a-zA-Z0-9]"),
                                        }
                                    } else {
                                        __state.mark_failure(__pos, "[_a-zA-Z0-9]")
                                    };
                                    __state.suppress_fail -= 1;
                                    match __assert_res {
                                        Failed => Matched(__pos, ()),
                                        Matched(..) => Failed,
                                    }
                                };
                                match __seq_res {
                                    Matched(__pos, _) => Matched(__pos, { e }),
                                    Failed => Failed,
                                }
                            }
                            Failed => Failed,
                        }
                    };
                    __state.suppress_fail -= 1;
                    res
                };
                match __seq_res {
                    Matched(__pos, _) => Matched(__pos, { StructKind::Union }),
                    Failed => Failed,
                }
            }
        }
    }
}

fn __parse_struct_declaration<'input>(__input: &'input str, __state: &mut ParseState<'input>, __pos: usize, env: &mut Env) -> RuleResult<StructDeclaration> {
    #![allow(non_snake_case, unused)]
    {
        let __choice_res = {
            let __seq_res = {
                let __seq_res = Matched(__pos, __pos);
                match __seq_res {
                    Matched(__pos, l) => {
                        let __seq_res = __parse_struct_field(__input, __state, __pos, env);
                        match __seq_res {
                            Matched(__pos, e) => {
                                let __seq_res = Matched(__pos, __pos);
                                match __seq_res {
                                    Matched(__pos, r) => Matched(__pos, { Node::new(e, Span::span(l, r)) }),
                                    Failed => Failed,
                                }
                            }
                            Failed => Failed,
                        }
                    }
                    Failed => Failed,
                }
            };
            match __seq_res {
                Matched(__pos, f) => Matched(__pos, { StructDeclaration::Field(f) }),
                Failed => Failed,
            }
        };
        match __choice_res {
            Matched(__pos, __value) => Matched(__pos, __value),
            Failed => {
                let __choice_res = {
                    let __seq_res = __parse_static_assert(__input, __state, __pos, env);
                    match __seq_res {
                        Matched(__pos, s) => Matched(__pos, { StructDeclaration::StaticAssert(s) }),
                        Failed => Failed,
                    }
                };
                match __choice_res {
                    Matched(__pos, __value) => Matched(__pos, __value),
                    Failed => {
                        let __seq_res = {
                            let __seq_res = {
                                __state.suppress_fail += 1;
                                let __assert_res = __parse_gnu_guard(__input, __state, __pos, env);
                                __state.suppress_fail -= 1;
                                match __assert_res {
                                    Matched(_, __value) => Matched(__pos, __value),
                                    Failed => Failed,
                                }
                            };
                            match __seq_res {
                                Matched(__pos, _) => {
                                    let __seq_res = {
                                        __state.suppress_fail += 1;
                                        let res = {
                                            let __seq_res = slice_eq(__input, __state, __pos, "__extension__");
                                            match __seq_res {
                                                Matched(__pos, e) => {
                                                    let __seq_res = {
                                                        __state.suppress_fail += 1;
                                                        let __assert_res = if __input.len() > __pos {
                                                            let (__ch, __next) = char_range_at(__input, __pos);
                                                            match __ch {
                                                                '_' | 'a'..='z' | 'A'..='Z' | '0'..='9' => Matched(__next, ()),
                                                                _ => __state.mark_failure(__pos, "[_a-zA-Z0-9]"),
                                                            }
                                                        } else {
                                                            __state.mark_failure(__pos, "[_a-zA-Z0-9]")
                                                        };
                                                        __state.suppress_fail -= 1;
                                                        match __assert_res {
                                                            Failed => Matched(__pos, ()),
                                                            Matched(..) => Failed,
                                                        }
                                                    };
                                                    match __seq_res {
                                                        Matched(__pos, _) => Matched(__pos, { e }),
                                                        Failed => Failed,
                                                    }
                                                }
                                                Failed => Failed,
                                            }
                                        };
                                        __state.suppress_fail -= 1;
                                        res
                                    };
                                    match __seq_res {
                                        Matched(__pos, e) => Matched(__pos, { e }),
                                        Failed => Failed,
                                    }
                                }
                                Failed => Failed,
                            }
                        };
                        match __seq_res {
                            Matched(__pos, _) => {
                                let __seq_res = __parse__(__input, __state, __pos, env);
                                match __seq_res {
                                    Matched(__pos, _) => {
                                        let __seq_res = __parse_struct_declaration(__input, __state, __pos, env);
                                        match __seq_res {
                                            Matched(__pos, d) => Matched(__pos, { d }),
                                            Failed => Failed,
                                        }
                                    }
                                    Failed => Failed,
                                }
                            }
                            Failed => Failed,
                        }
                    }
                }
            }
        }
    }
}

fn __parse_struct_field<'input>(__input: &'input str, __state: &mut ParseState<'input>, __pos: usize, env: &mut Env) -> RuleResult<StructField> {
    #![allow(non_snake_case, unused)]
    {
        let __seq_res = __parse_specifier_qualifiers(__input, __state, __pos, env);
        match __seq_res {
            Matched(__pos, s) => {
                let __seq_res = __parse__(__input, __state, __pos, env);
                match __seq_res {
                    Matched(__pos, _) => {
                        let __seq_res = {
                            let __seq_res = {
                                let mut __repeat_pos = __pos;
                                let mut __repeat_value = vec![];
                                loop {
                                    let __pos = __repeat_pos;
                                    let __pos = if __repeat_value.len() > 0 {
                                        let __sep_res = {
                                            let __seq_res = __parse__(__input, __state, __pos, env);
                                            match __seq_res {
                                                Matched(__pos, _) => {
                                                    let __seq_res = slice_eq(__input, __state, __pos, ",");
                                                    match __seq_res {
                                                        Matched(__pos, _) => __parse__(__input, __state, __pos, env),
                                                        Failed => Failed,
                                                    }
                                                }
                                                Failed => Failed,
                                            }
                                        };
                                        match __sep_res {
                                            Matched(__newpos, _) => __newpos,
                                            Failed => break,
                                        }
                                    } else {
                                        __pos
                                    };
                                    let __step_res = {
                                        let __seq_res = Matched(__pos, __pos);
                                        match __seq_res {
                                            Matched(__pos, l) => {
                                                let __seq_res = __parse_struct_declarator(__input, __state, __pos, env);
                                                match __seq_res {
                                                    Matched(__pos, e) => {
                                                        let __seq_res = Matched(__pos, __pos);
                                                        match __seq_res {
                                                            Matched(__pos, r) => Matched(__pos, { Node::new(e, Span::span(l, r)) }),
                                                            Failed => Failed,
                                                        }
                                                    }
                                                    Failed => Failed,
                                                }
                                            }
                                            Failed => Failed,
                                        }
                                    };
                                    match __step_res {
                                        Matched(__newpos, __value) => {
                                            __repeat_pos = __newpos;
                                            __repeat_value.push(__value);
                                        }
                                        Failed => {
                                            break;
                                        }
                                    }
                                }
                                Matched(__repeat_pos, __repeat_value)
                            };
                            match __seq_res {
                                Matched(__pos, e) => Matched(__pos, { e }),
                                Failed => Failed,
                            }
                        };
                        match __seq_res {
                            Matched(__pos, d) => {
                                let __seq_res = __parse__(__input, __state, __pos, env);
                                match __seq_res {
                                    Matched(__pos, _) => {
                                        let __seq_res = slice_eq(__input, __state, __pos, ";");
                                        match __seq_res {
                                            Matched(__pos, _) => {
                                                let __seq_res = match {
                                                    let __seq_res = {
                                                        __state.suppress_fail += 1;
                                                        let __assert_res = __parse_gnu_guard(__input, __state, __pos, env);
                                                        __state.suppress_fail -= 1;
                                                        match __assert_res {
                                                            Matched(_, __value) => Matched(__pos, __value),
                                                            Failed => Failed,
                                                        }
                                                    };
                                                    match __seq_res {
                                                        Matched(__pos, _) => {
                                                            let __seq_res = {
                                                                let mut __repeat_pos = __pos;
                                                                let mut __repeat_value = vec![];
                                                                loop {
                                                                    let __pos = __repeat_pos;
                                                                    let __step_res = {
                                                                        let __seq_res = __parse__(__input, __state, __pos, env);
                                                                        match __seq_res {
                                                                            Matched(__pos, _) => slice_eq(__input, __state, __pos, ";"),
                                                                            Failed => Failed,
                                                                        }
                                                                    };
                                                                    match __step_res {
                                                                        Matched(__newpos, __value) => {
                                                                            __repeat_pos = __newpos;
                                                                            __repeat_value.push(__value);
                                                                        }
                                                                        Failed => {
                                                                            break;
                                                                        }
                                                                    }
                                                                }
                                                                Matched(__repeat_pos, __repeat_value)
                                                            };
                                                            match __seq_res {
                                                                Matched(__pos, e) => Matched(__pos, { e }),
                                                                Failed => Failed,
                                                            }
                                                        }
                                                        Failed => Failed,
                                                    }
                                                } {
                                                    Matched(__newpos, _) => Matched(__newpos, ()),
                                                    Failed => Matched(__pos, ()),
                                                };
                                                match __seq_res {
                                                    Matched(__pos, _) => Matched(__pos, { StructField { specifiers: s, declarators: d } }),
                                                    Failed => Failed,
                                                }
                                            }
                                            Failed => Failed,
                                        }
                                    }
                                    Failed => Failed,
                                }
                            }
                            Failed => Failed,
                        }
                    }
                    Failed => Failed,
                }
            }
            Failed => Failed,
        }
    }
}
