use crate::analyzer::{Analyzer, LocalId, ObjectInfo, ResolvedAttachment, ResolvedFunctionBody, StaticInfo};
use crate::intrinsic::IntrinsicParameters;
use crate::{AzimuthId, CallStackFunction, Function, FunctionParameter, MappingTo, NumKind};
use crate::lexer::{Operator, Span};
use crate::{
    Mapping, ObjectId, Number, Runtime, ShapeId, Value, ValueKind, executor,
    analyzer::{ResolvedExpression, ResolvedShapeExpression, ResolvedStatement, Symbol},
};
use std::collections::{HashMap, HashSet};
use std::{fs, usize};

#[derive(Debug, Clone)]
pub enum RuntimeError {
    Error { span: Span, message: String },
    Throw { span: Span, message: String },
    TypeMismatch { span: Span, found: Value, expected: ValueKind },
    UnexpectedBreakout { span: Span },
    InvalidOperator { span: Span, operator: Operator, operand: ValueKind },
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuntimeError::Error { span, message } =>
                write!(f, "{}: {}", span, message),
            RuntimeError::Throw { span, message } =>
                write!(f, "{}: Throw: \"{}\"", span, message),
            RuntimeError::TypeMismatch { span, found, expected } =>
                write!(f, "{}: {:?} does not match expected type {:?}", span, found, expected),
            RuntimeError::UnexpectedBreakout { span } =>
                write!(f, "{}: Unexpected breakout", span),
            RuntimeError::InvalidOperator { span, operator, operand } =>
                write!(f, "{}: {:?} is invalid operator for {:?}", span, operator, operand),
        }
    }
}

macro_rules! numeric_binop {
    ($span:expr, $left:expr, $right:expr, $operator:expr, $t:ty) => {{
        let left = $left as $t;
        let right = $right as $t;
        match $operator {
            Operator::Equal => Ok((left == right).into()),
            Operator::NEqual => Ok((left != right).into()),
            Operator::LT => Ok((left < right).into()),
            Operator::GT => Ok((left > right).into()),
            Operator::LTE => Ok((left <= right).into()),
            Operator::GTE => Ok((left >= right).into()),
            
            Operator::Add => Ok((left + right).into()),
            Operator::Sub => Ok((left - right).into()),
            Operator::Mul => Ok((left * right).into()),
            Operator::Div => Ok((left / right).into()),
            Operator::Mod => Ok((left % right).into()),

            Operator::BWAnd => Ok((left & right).into()),
            Operator::BWOr => Ok((left | right).into()),
            Operator::BWXor => Ok((left ^ right).into()),
            Operator::BWShiftL => Ok((left << right).into()),
            Operator::BWShiftR => Ok((left >> right).into()),
            
            Operator::Range => Ok(create_range(left.into(), right.into(), true)),
            Operator::RangeLT => Ok(create_range(left.into(), right.into(), false)),
            
            operator => Err(RuntimeError::InvalidOperator { span:$span, operator, operand: ValueKind::Number(NumKind::Any) }),
        }
    }};
}

macro_rules! float_binop {
    ($span:expr, $left:expr, $right:expr, $operator:expr, $t:ty) => {{
        let left = $left as $t;
        let right = $right as $t;
        match $operator {
            Operator::Equal => Ok((left == right).into()),
            Operator::NEqual => Ok((left != right).into()),
            Operator::LT => Ok((left < right).into()),
            Operator::GT => Ok((left > right).into()),
            Operator::LTE => Ok((left <= right).into()),
            Operator::GTE => Ok((left >= right).into()),
            
            Operator::Add => Ok((left + right).into()),
            Operator::Sub => Ok((left - right).into()),
            Operator::Mul => Ok((left * right).into()),
            Operator::Div => Ok((left / right).into()),
            Operator::Mod => Ok((left % right).into()),
 
            operator => Err(RuntimeError::InvalidOperator { span:$span, operator, operand: ValueKind::Number(NumKind::Any) }),
        }
    }};
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct ShapeInstance {
    pub id: ShapeId,
    pub generics: Vec<ValueKind>,
}

pub const OBJECT_INSTANCE: ShapeInstance = ShapeInstance{id:0, generics:Vec::new()};

pub fn evaluate_shape(runtime: &Runtime, shape:ResolvedShapeExpression) -> ValueKind {
    shape.kind()
}

pub fn evaluate(runtime: &mut Runtime, expression:ResolvedExpression) -> Result<Value, RuntimeError> {
    match expression {
        ResolvedExpression::Value(_, value) => Ok(value),
        ResolvedExpression::Array(_, expressions, kind) => {
            let mut values = Vec::new();
            for item in expressions {
                values.push(evaluate(runtime, item)?);
            }
            Ok(Value::Array(values, kind))
        },
        ResolvedExpression::Set(_, expressions, kind) => {
            let mut values = HashSet::new();
            for item in expressions {
                values.insert(evaluate(runtime, item)?);
            }
            Ok(Value::Set(values.iter().cloned().collect(), kind))
        },
        ResolvedExpression::Dict(_, expressions, key_kind, value_kind) => {
            let mut values = HashMap::new();
            for (key, val) in expressions {
                values.insert(evaluate(runtime, key)?, evaluate(runtime, val)?);
            }
            Ok(Value::Dict(values.iter().map(|(k,v)| (k.clone(),v.clone())).collect(), key_kind, value_kind))
        },
        ResolvedExpression::Variable(span, Symbol::Generic(k)) => return Err(RuntimeError::Error{span, message:format!("Somehow generic made it through: {:?}", k)}),
        ResolvedExpression::Variable(span, Symbol::Local(k)) => { 
            //println!("{:?}, toget: {}", runtime.locals, k.id);
            match runtime.get_local(k.id) {
                Some(val) => Ok(val.clone()),
                None => Err(RuntimeError::Error{span, message: format!("Missing local: {:?}", k)})
            }
        }

        ResolvedExpression::Default(span, kind) => {
            let value = match kind {
                // Primitives
                ValueKind::String => format!("").into(),
                ValueKind::Bool => false.into(),
                ValueKind::None => Value::None,
                ValueKind::Number(NumKind::Int8) => 0i8.into(),
                ValueKind::Number(NumKind::Int16) => 0i16.into(),
                ValueKind::Number(NumKind::Int32) => 0i32.into(),
                ValueKind::Number(NumKind::Int64) => 0i64.into(),
                ValueKind::Number(NumKind::UInt8) => 0u8.into(),
                ValueKind::Number(NumKind::UInt16) => 0u16.into(),
                ValueKind::Number(NumKind::UInt32) => 0u32.into(),
                ValueKind::Number(NumKind::UInt64) => 0u64.into(),
                ValueKind::Number(NumKind::Float32) => 0f32.into(),
                ValueKind::Number(NumKind::Float64) => 0f64.into(),

                // Collections
                ValueKind::Array(kind) => Value::Array([].into(), *kind),
                ValueKind::Set(kind) => Value::Set([].into(), *kind),
                ValueKind::Dict(key_kind, value_kind) => Value::Dict([].into(), *key_kind, *value_kind),

                // Other
                ValueKind::Option(_) => Value::None,
                
                // Object
                ValueKind::Object(kinds) => {
                    let expr = ResolvedExpression::ObjectInit(span.clone(), ResolvedAttachment{
                        defaults:Vec::new(), 
                        mappings:Vec::new(), 
                        base:OBJECT_INSTANCE,
                        known:kinds,
                    });
                    evaluate(runtime, expr)?
                }
                ValueKind::Shape(inst) => {
                    let expr = ResolvedExpression::ObjectInit(span.clone(), ResolvedAttachment{
                        defaults:Vec::new(), 
                        mappings:Vec::new(), 
                        base:inst.clone(),
                        known:[ValueKind::Shape(inst)].to_vec(),
                    });
                    evaluate(runtime, expr)?
                }

                other => return Err(RuntimeError::Error{span, message:format!("Could not determine default for {:?}", other)})
            };

            Ok(value)
        }

        ResolvedExpression::StaticSingleton(span, info) => {
            let shape = match runtime.get_shape(info) {
                None => return Err(RuntimeError::Error{span, message:format!("No shape found for static id: {:?}", info)}),
                Some(info) => info,
            };

            let static_info = match &shape.static_id {
                None => return Err(RuntimeError::Error{span, message:format!("{:?} does not have static singleton", shape.name)}),
                Some(id) => id
            };

            Ok(Value::Object(static_info.id, ValueKind::Object([].to_vec())))
        },
        
        ResolvedExpression::Option(span,_) => todo!(),
        ResolvedExpression::Shape(span,_) => todo!(),
        ResolvedExpression::Reflection(span,_) => todo!(),

        ResolvedExpression::ObjectInit(span, attachment) => {
            let name = format!("Obj{}", runtime.next_object_id);
            let id = runtime.create_object(name);

            let known = ValueKind::Shape(attachment.base.clone());
            let mut known_shapes = [known].to_vec();

            let shape = runtime.get_shape(attachment.base.id);
            match shape {
                Some(info) => {
                    for parent in &info.parents {
                        known_shapes.push(ValueKind::Shape(parent.base.clone()));
                    }
                    runtime.attach_shape(span.clone(), id, attachment)?;
                }
                None => return Err(RuntimeError::Error{span, message:format!("Shape not found: {:?}", attachment.base)})
            }

            let kind = ValueKind::Object(known_shapes);
            let object = Value::Object(id, kind);

            Ok(object)
        }
        
        ResolvedExpression::StringFormat(span, expressions) => {
            let mut string = String::new();
            for expr in expressions {
                match evaluate(runtime, expr)? {
                    Value::String(s) => string += &s,
                    other => string += &other.to_string(),
                }
            }

            Ok(Value::String(string))
        }

        ResolvedExpression::UnaryOp { span, operator, operand } => {
            match (operator, evaluate(runtime, *operand)?) {
                (op, Value::Bool(val)) => 
                    match op {
                        Operator::Not => Ok((!val).into()),
                        operator => Err(RuntimeError::InvalidOperator { span, operator, operand:ValueKind::Bool })
                    },
                (op, Value::Number(val)) => {
                    let val = val.to_i64().unwrap();
                    match op {
                        Operator::Inc => Ok((val + 1).into()),
                        Operator::Dec => Ok((val - 1).into()),
                        Operator::BWNot => Ok((!val).into()),
                        Operator::Sub => Ok((-val).into()),
                        operator => Err(RuntimeError::InvalidOperator { span, operator, operand:ValueKind::Number(NumKind::Int32) })
                    }
                },
                (op, Value::String(val)) => 
                    match op {
                        Operator::Len => Ok((val.chars().count() as u64).into()),
                        operator => Err(RuntimeError::InvalidOperator { span, operator, operand:ValueKind::String })
                    },
                (op, Value::Array(vec, val)) => 
                    match op {
                        Operator::Len => Ok((vec.len() as u64).into()),
                        operator => Err(RuntimeError::InvalidOperator { span, operator, operand:ValueKind::Array(Box::new(val)) })
                    },
                (op, Value::Set(vec, val)) => 
                    match op {
                        Operator::Len => Ok((vec.len() as u64).into()),
                        operator => Err(RuntimeError::InvalidOperator { span, operator, operand:ValueKind::Set(Box::new(val)) })
                    },
                (op, Value::Dict(vec, key, val)) => 
                    match op {
                        Operator::Len => Ok((vec.len() as u64).into()),
                        operator => Err(RuntimeError::InvalidOperator { span, operator, operand:ValueKind::Dict(Box::new(key),Box::new(val)) })
                    },

                (operator, operand) => Err(RuntimeError::Error{ span, message: format!("Invalid operation: {:?} {:?}", operator, operand) })
            }
        },

        ResolvedExpression::BinaryOp { span, left, operator, right } => {
            match (evaluate(runtime, *left)?, operator, right.kind()) { 
 
                // Option ??
                (left, Operator::DQuestion, _) => {
                    match left.kind() {
                        ValueKind::None => Ok(evaluate(runtime, *right)?),
                        _ => Ok(left)
                    }
                }
                
                // Bool - Bool
                (Value::Bool(left), op, ValueKind::Bool) => 
                    match op {
                        Operator::Equal => {
                            Ok((left == match evaluate(runtime, *right)? {
                                Value::Bool(val) => val,
                                _ => unreachable!()
                            }).into())
                        }
                        Operator::NEqual => {
                            Ok((left != match evaluate(runtime, *right)? {
                                Value::Bool(val) => val,
                                _ => unreachable!()
                            }).into())
                        }
                        Operator::And => {
                            Ok((left && match evaluate(runtime, *right)? {
                                Value::Bool(val) => val,
                                _ => unreachable!()
                            }).into())
                        }
                        Operator::Or => {
                            Ok((left || match evaluate(runtime, *right)? {
                                Value::Bool(val) => val,
                                _ => unreachable!()
                            }).into())
                        }
                        operator => Err(RuntimeError::InvalidOperator { span, operator, operand: ValueKind::Bool }),
                    },
                      
                // Int - Int
                (Value::Number(left), op, r) if r.is_assignable_from(ValueKind::Number(NumKind::Any)) => {
                    let right = match evaluate(runtime, *right)? {
                        Value::Number(val) => val,
                        _ => unreachable!()
                    };
                    let kind = Number::promote_kind(left.num_kind(), right.num_kind());
                    match kind {
                        NumKind::Float64 => float_binop!(span, left.to_f64().unwrap(), right.to_f64().unwrap(), op, f64),
                        NumKind::Float32 => float_binop!(span, left.to_f32().unwrap(), right.to_f32().unwrap(), op, f32),
                        NumKind::UInt64  => numeric_binop!(span, left.to_u64().unwrap(), right.to_u64().unwrap(), op, u64),
                        NumKind::Int64   => numeric_binop!(span, left.to_i64().unwrap(), right.to_i64().unwrap(), op, i64),
                        NumKind::UInt32  => numeric_binop!(span, left.to_u32().unwrap(), right.to_u32().unwrap(), op, u32),
                        NumKind::Int32   => numeric_binop!(span, left.to_i32().unwrap(), right.to_i32().unwrap(), op, i32),
                        NumKind::UInt16  => numeric_binop!(span, left.to_u16().unwrap(), right.to_u16().unwrap(), op, u16),
                        NumKind::Int16   => numeric_binop!(span, left.to_i16().unwrap(), right.to_i16().unwrap(), op, i16),
                        NumKind::UInt8  => numeric_binop!(span, left.to_u8().unwrap(), right.to_u8().unwrap(), op, u8),
                        NumKind::Int8   => numeric_binop!(span, left.to_i8().unwrap(), right.to_i8().unwrap(), op, i8),
                        NumKind::Any   => numeric_binop!(span, left.to_i32().unwrap(), right.to_i32().unwrap(), op, i32),
                        _ => return Err(RuntimeError::Error{span:span.clone(), message:format!("Couldnt do number conversion: {:?} to {:?}", left, right)})
                    }
                }
                    
                // String - String
                (Value::String(left), op, ValueKind::String) => {
                    let right = match evaluate(runtime, *right)? {
                        Value::String(val) => val,
                        _ => unreachable!()
                    };
                    match op {
                        Operator::Equal => Ok((left == right).into()),
                        Operator::NEqual => Ok((left != right).into()),
                        
                        Operator::Add => Ok((left + &right).into()),
                        
                        operator => Err(RuntimeError::InvalidOperator { span, operator, operand: ValueKind::String }),
                    }
                }

                // Equality
                (left, op, _) if matches!(op, Operator::Equal | Operator::NEqual) => {
                    let right = evaluate(runtime, *right)?;
                    match op {
                        Operator::Equal => Ok((left == right).into()),
                        Operator::NEqual => Ok((left != right).into()),
                        
                        operator => Err(RuntimeError::InvalidOperator { span, operator, operand: left.kind() }),
                    }
                }

                // IsShape and NisShape
                (Value::Object(id, _), op, ValueKind::Shape(inst)) => {
                    match op {
                        Operator::IsShape => Ok((runtime.is_shape(id, inst.id)).into()),
                        Operator::NIsShape => Ok((!runtime.is_shape(id, inst.id)).into()),
                        
                        operator => Err(RuntimeError::InvalidOperator { span, operator, operand: ValueKind::Shape(inst) }),
                    }
                }

                (left, op, right) => Err(RuntimeError::Error{ span, message: format!("Invalid operation: {:?} {:?} {:?}", left, op, right) })
            }
        },

        ResolvedExpression::Ternary { span, condition, true_expr, else_expr  } => {
            let condition = evaluate(runtime, *condition)?;
            if condition.kind() != ValueKind::Bool {
                return Err(RuntimeError::TypeMismatch { span, found: condition, expected: ValueKind::Bool })
            }

            if condition == Value::Bool(true) {
                Ok(evaluate(runtime,*true_expr)?)
            } else {
                Ok(evaluate(runtime,*else_expr)?)
            }
        }
        
        ResolvedExpression::MemberAccess{ span, target, member, optional, chained} => {
            //println!("Target: {:?}", target);

            let object_id = match evaluate(runtime, *target)? {
                Value::Object(id, _) => id,
                Value::None if chained => return Ok(Value::None),
                other => {
                    match runtime.get_intrinsic_static_id(other.kind()) {
                        None => return Err(RuntimeError::Error{span, message:format!("Member access not permitted for {:?}.{:?}", other, member)}),
                        Some(id) => id
                    }
                }
            };

            match runtime.get_slot_value(object_id, member.id) {
                Some(value) => Ok(value.clone()),
                None if optional => Ok(Value::None),
                None => Err(RuntimeError::Error{span, message:format!("Member {:?} not found for {:?}", member.name, object_id)}),
            }
        },
        
        ResolvedExpression::ArrayAccess{ span, target, index, optional, chained} => {
            let target = evaluate(runtime, *target)?;
            let index = evaluate(runtime, *index)?;

            match (target, index) {
                (Value::Array(array, _), Value::Number(index)) => {
                    let index = index.to_i64().unwrap();
                    let i = if index < 0 {
                        ((array.len() as i64) + index) as usize
                    } else { index as usize };

                    match array.get(i) {
                        Some(value) => Ok(value.clone()),
                        None if optional => Ok(Value::None),
                        None => return Err(RuntimeError::Error{span, message:format!("Index {:?} out of bounds (len of {})", index, array.len())}),
                    }
                }
                (Value::String(string), Value::Number(index)) => {
                    //println!("Accessing string: {}", string);
                    let chars: Vec<String> = string.chars().map(|k|k.to_string()).collect();
                    match chars.get(index.to_u64().unwrap() as usize) {
                        Some(value) => Ok(value.clone().into()),
                        None if optional => Ok(Value::None),
                        None => return Err(RuntimeError::Error{span, message:format!("Index {:?} out of bounds ({:?})", index, string.chars().count() - 1)}),
                    }
                }
                (Value::Range(start, end, by, inclusive, kind), Value::Number(index)) => {
                    //println!("GOD IS GOOD");
                    let start = start.to_i32().unwrap();
                    let end = end.to_i32().unwrap();
                    let by = by.to_i32().unwrap();
                    let index = index.to_i32().unwrap();

                    let num = start + (by * index);
                    if (inclusive && num >= end) || num > end {
                        return Err(RuntimeError::Error{span, message:format!("Index {:?} out of bounds ({:?})", index, end)})
                    }

                    Ok(Value::Number(Number::Int32(num)))
                }
                (Value::None, _) if chained => Ok(Value::None),
                (other, member) => Err(RuntimeError::Error{span, message:format!("Array access not permitted for {:?}.{:?}", other, member)}),
            }
        },
        
        ResolvedExpression::FunctionCall{ span, target, args, optional, chained} => {
            let mut params = Vec::new();
            for arg in &args {
                params.push(evaluate(runtime, arg.clone())?);
            }

            let evaluated_target = evaluate(runtime, *target.clone())?;

            let func = match evaluated_target {
                Value::Function(func) => func,

                Value::FunctionChain(azimuths, kind) => {
                    let object_id = match *target {

                        ResolvedExpression::MemberAccess { target, .. } => {
                            match evaluate(runtime, *target)? {
                                Value::Object(id, _) => id,
                                _ => return Err(RuntimeError::Error{span, message:format!("FUCK 2")})
                            }
                        }
                        _ => return Err(RuntimeError::Error{span, message:format!("FUCK 1")})
                    };

                    // Make individual function calls for each function
                    let mut value = Value::None;
                    for mapping in azimuths {
                        let found = match mapping {
                            MappingTo::Slot(id) => {
                                let object = match runtime.get_object(object_id) {
                                    Some(obj) => obj,
                                    None => panic!("OBJNOTFOUND 1")
                                };
                                if let Some(state) = object.get_slot_state(id) {
                                    Some(state.storage.clone())
                                } else {
                                    todo!()
                                }
                            }
                            MappingTo::Link(other_object_id, other_azimuth) => {
                                runtime.get_slot_value(other_object_id, other_azimuth)
                            }
                            MappingTo::Map(other_azimuth) => {
                                runtime.get_slot_value(object_id, other_azimuth)
                            }
                            MappingTo::Chain(azimuths, _) => {
                                Some(Value::FunctionChain(azimuths.clone(), kind.clone()))
                            }
                            MappingTo::Expression(expr) => {
                                let result = evaluate_place(runtime, expr)?;
                                Some(result)
                            }
                        };

                        let function = match found {
                            None => return Err(RuntimeError::Error{span, message:format!("FUCK 3")}),
                            Some(func) => func,
                        };

                        let function_call = ResolvedExpression::FunctionCall{ span:span.clone(), 
                            target:Box::new(ResolvedExpression::Value(span.clone(), function.clone())), 
                            args:args.clone(), optional, chained 
                        };

                        value = evaluate(runtime, function_call)?;
                    }
                    return Ok(value);
                },

                Value::None if chained => return Ok(Value::None),
                other => return Err(RuntimeError::Error{span, message:format!("{:?} is not a function or function chain", other)}),
            };

            match func.func.as_ref() {
                None => Err(RuntimeError::Error{ span, message:format!("Abstract method run without body") }),
                Some(ResolvedFunctionBody::Script(statement)) => {
                    let expected_return = func.output_type.clone();

                    // Create locals
                    for i in 0..func.input_types.len() {
                        let param = params.get(i).unwrap();
                        let local = func.input_types.get(i).unwrap().local;
                        
                        match local {
                            Some(id) => runtime.reserve_local(id, param.clone()),
                            None => {}
                        }
                       
                    }
                    for capture in &func.captures {
                        runtime.ref_local(*capture, 0);
                    }
                    
                    // Add to stack
                    runtime.call_stack.push(CallStackFunction{
                        id:func.id,
                        span: span.clone(),
                        arguments: params.clone()
                    });

                    match execute_statement(runtime, statement.clone())? {
                        ExecFlow::Error { span, message } => Err(RuntimeError::Throw { span, message }),
                        ExecFlow::Return(span, value) => {
                            if !value.kind().is_assignable_from(expected_return.clone()) {
                                return Err(RuntimeError::TypeMismatch { span, found: value, expected: expected_return });
                            }

                            // Save Object
                            match value {
                                Value::Object(id, _) => {
                                    runtime.ref_obj(id);
                                }
                                _ => {}
                            }

                            // Free locals
                            for param in func.input_types {
                                match param.local {
                                    Some(id) => runtime.deref_local(id, 3),
                                    None => {}
                                }
                            }
                            for capture in &func.captures {
                                runtime.deref_local(*capture, 0);
                            }

                            // Remove from stack
                            runtime.call_stack.pop();

                            Ok(value.convert_to(func.output_type).unwrap())
                        },
                        ExecFlow::Normal(span) => {
                            if expected_return != ValueKind::None {
                                return Err(RuntimeError::Error { span, message:format!("No value returned, expected {:?}", expected_return) });
                            }

                            // Free locals
                            for param in func.input_types {
                                match param.local {
                                    Some(id) => runtime.deref_local(id, 4),
                                    None => {}
                                }
                            }
                            
                            // Remove from stack
                            runtime.call_stack.pop();

                            Ok(Value::None)
                        }
                        _ => Err(RuntimeError::Error { span, message:format!("No value returned, expected {:?}", expected_return) }),
                    }
                }
                Some(ResolvedFunctionBody::Intrinsic(func)) => {
                    let input = IntrinsicParameters{
                        span, args:params, runtime, azimuth:None
                    };
                    Ok(func(input)?)
                }
            }
        },

        ResolvedExpression::Function{ span, has_self, input_types, output_type, func, captures } => {
            let output_type = evaluate_shape(runtime, output_type);
            let mut inputs = Vec::new();
            for input in input_types {
                let kind = input.shape;
                let local = input.local;

                inputs.push(FunctionParameter{kind, local});
            }
            
            let function = Function{ 
                id: 0, 
                has_self, 
                input_types:inputs, 
                output_type, 
                func: *func,
                captures
            };
            Ok(Value::Function(Box::new(function)))
        }

        //other => panic!("Invalid expression: {:?}", other)
    }
}

pub fn evaluate_place(runtime: &mut Runtime, expression:ResolvedExpression) -> Result<Value, RuntimeError> {
    match expression {
        ResolvedExpression::MemberAccess{ span, target, member, optional, chained} => {
            match evaluate(runtime, *target)? {
                Value::Object(object_id, kind) => {
                    Ok(Value::Pointer(object_id, member.id, kind))
                }
                Value::None if chained => Ok(Value::None),
                other => Err(RuntimeError::Error{span, message:format!("Member access not permitted for {:?}.{:?}", other, member)}),
            }
        },
        ResolvedExpression::ArrayAccess{ span, target, index, optional, chained} => {
            let target = evaluate_place(runtime, *target)?;

            let (access_kind, value_kind) = match target.kind() {
                ValueKind::Array(kind) => (ValueKind::Number(NumKind::Any), *kind),
                ValueKind::Dict(k_kind,v_kind) => (*k_kind, *v_kind),
                ValueKind::None if chained => return Ok(Value::None),
                other => return Err(RuntimeError::Error{span, message:format!("Array access not permitted for {:?}", other)}),
            };

            let access = evaluate(runtime, *index)?;
            if !access.kind().is_assignable_from(access_kind.clone()) {
                return Err(RuntimeError::TypeMismatch{span, found:access, expected:access_kind})
            }
            
            Ok(Value::Element(Box::new(target), Box::new(access), value_kind))
        },
        ResolvedExpression::Variable(_, Symbol::Local(k)) => Ok(Value::Local(k.id, runtime.get_local(k.id).unwrap().kind())),
        other => evaluate(runtime, other), 
    }
}

pub fn create_range(from: Number, to: Number, inclusive: bool) -> Value {
    let kind = from.num_kind();
    let by = match kind {
        NumKind::Int8 | NumKind::Int16 | NumKind::Int32 | NumKind::Int64 => {
            let from = from.to_i64();
            let to = to.to_i64();
            if from <= to { 1 } else { -1 }
        }
        _ => 1,
    };
    Value::Range(from, to, Number::Any(by.into()), inclusive, kind)
}

pub fn execute(runtime: &mut Runtime, ast: Vec<ResolvedStatement>, static_info: HashMap<ShapeId, (StaticInfo, Vec<AzimuthId>)>) -> Result<ExecFlow, RuntimeError> {
    runtime.init_static_instances(static_info)?;

    for statement in ast {
        match execute_statement(runtime, statement)? {
            ExecFlow::Normal(_) => {},
            ExecFlow::Declare(_, _) => {},
            ExecFlow::Break(span) => {
                return Err(RuntimeError::UnexpectedBreakout{span});
            },
            ExecFlow::Continue(span) => {
                return Err(RuntimeError::UnexpectedBreakout{span});
            },
            ExecFlow::Return(span, _) => {
                return Err(RuntimeError::UnexpectedBreakout{span});
            },
            ExecFlow::Error { span, message } => {
                return Err(RuntimeError::Throw{span, message});
            }
        }
    }
    Ok(ExecFlow::Normal(Span::new(0,0,format!("Runtime"))))
}

#[derive(Debug, Clone)]
pub enum ExecFlow {
    Normal(Span),
    Break(Span),
    Continue(Span),
    Return(Span, Value),
    Declare(Span, LocalId),
    Error{span:Span, message:String},
}

pub fn execute_statement(runtime: &mut Runtime, statement: ResolvedStatement) -> Result<ExecFlow, RuntimeError> {
    match statement {
        ResolvedStatement::Expression { span, expr } => { 
            evaluate(runtime, expr)?;
            Ok(ExecFlow::Normal(span))
        },
        ResolvedStatement::Print { span, expr } => {
            match evaluate(runtime, expr)? {
                Value::String(k) => println!("{}", k),
                Value::Object(object_id, _) => runtime.print_object(object_id),
                other => println!("{}", other.to_string()),
            }
            Ok(ExecFlow::Normal(span))
        },

        ResolvedStatement::DeclareLocal { span, info, value } => {
            let id = info.id.clone();
            let value = evaluate(runtime, value)?;

            runtime.reserve_local(id, value);
            
            Ok(ExecFlow::Declare(span, id))
        },

        ResolvedStatement::Detach { span, object, shape } => {
            match (evaluate(runtime, object)?, evaluate_shape(runtime, shape)) {
                (Value::Object(object_id, _), ValueKind::Shape(shape_inst)) => {
                    let object = match runtime.get_object(object_id) {
                        Some(obj) => obj,
                        None => panic!("OBJNOTFOUND 3")
                    };
                    let sealed = object.flags.sealed;
                    if !sealed {
                        runtime.detach_shape(span.clone(), object_id, shape_inst)?
                    }
                }
                (object, shape) => return Err(RuntimeError::Error{span, message:format!("Could not detach {:?} from {:?}", shape, object)})
            }
            Ok(ExecFlow::Normal(span))
        },

        ResolvedStatement::AddMapping { span, object, mapping } => {
            match evaluate(runtime, object)? {
                Value::Object(object_id, _) => {
                        let object = match runtime.get_object(object_id) {
                            Some(obj) => obj,
                            None => panic!("OBJNOTFOUND 3")
                        };
                        let sealed = object.flags.sealed;
                        if !sealed {
                            runtime.remap_slot(span.clone(), object_id, mapping.to.id, mapping.from.id)?
                        }
                    }
                object => return Err(RuntimeError::Error{span, message:format!("Invalid mapping: {:?}, {:?} -> {:?}", object, mapping.from, mapping.to)}),
            }
            Ok(ExecFlow::Normal(span))
        },

        ResolvedStatement::Attach { span, object, attachment } => {
            match evaluate(runtime, object)? {
                Value::Object(object_id, _) => {
                    let object = match runtime.get_object(object_id) {
                        Some(obj) => obj,
                        None => panic!("OBJNOTFOUND 2")
                    };
                    let sealed = object.flags.sealed;
                    if sealed { return Ok(ExecFlow::Normal(span)) }

                    runtime.attach_shape(span.clone(), object_id, attachment)?;
                },
                object => return Err(RuntimeError::Error{span, message:format!("Could not attach {:?} to {:?}", attachment.base, object)})
            }
            Ok(ExecFlow::Normal(span))
        },
        
        ResolvedStatement::Assign { span, target, value } => {
            let val = evaluate(runtime, value)?;

            match evaluate_place(runtime, target)? {
                Value::Pointer(object_id, az, kind) => {
                    runtime.set_slot_value(span.clone(), object_id, az, val)?;
                }
                Value::Element(target, access, kind) => {
                    //runtime.set_slot_value_array_element(obj, az, i, val);
                }
                Value::Local(loc, kind) => {
                    runtime.reserve_local(loc, val);
                }
                other => return Err(RuntimeError::Error{span, message:format!("Could not assign {:?} to {:?}", val, other)}),
            }
            Ok(ExecFlow::Normal(span))
        }
        ResolvedStatement::Seal { span, target } => {
            let name = target.get_name().clone();
            let object = evaluate(runtime, target)?;

            match object {
                Value::Object(id, _) => {
                    runtime.seal(id);
                    Ok(ExecFlow::Normal(span))
                },
                _ => Err(RuntimeError::Error { span, message: format!("{} is not sealable", name) })
            }
        }

        ResolvedStatement::If { span, condition, true_statement, else_statement } => {
            let kind = condition.kind();
            let cond = evaluate(runtime, condition)?;

            match (kind, cond) {
                (ValueKind::Bool, Value::Bool(false)) | (ValueKind::Option(_), Value::None) => {
                    if let Some(statement) = else_statement {
                        return execute_statement(runtime, *statement)
                    }
                    Ok(ExecFlow::Normal(span))
                }
                (ValueKind::Bool, Value::Bool(true)) | (ValueKind::Option(_), _) => execute_statement(runtime, *true_statement),
                (_, other) => Err(RuntimeError::Error{span, message:format!("If condition was not true or false: {:?}", other)}),
            }
        }

        ResolvedStatement::Switch { span, target, branch_statements, else_statement } => {
            let target = evaluate(runtime, target)?;

            let mut fell_thru = false;

            for (expr, cont, statement) in branch_statements {
                let comparison = evaluate(runtime, expr)?;
                if target != comparison { continue }

                let branch_result = execute_statement(runtime, statement)?;

                match branch_result {
                    ExecFlow::Normal(_) if cont => {
                        fell_thru = true;
                        continue
                    }
                    ExecFlow::Declare(_, loc) if cont => {
                        fell_thru = true;
                        runtime.deref_local(loc, 52);
                        continue
                    }
                    other => return Ok(other)
                }
            }

            match else_statement {
                Some(statement) if !fell_thru => execute_statement(runtime, *statement),
                _ => Ok(ExecFlow::Normal(span)),
            }
        }

        ResolvedStatement::Try { span, try_statement, catch_statement } => {
            let result = execute_statement(runtime, *try_statement);
            match (&result, *catch_statement) {
                (Err(_), Some(catch_statement)) => execute_statement(runtime, catch_statement),
                (Err(_), _) => Ok(ExecFlow::Normal(span)),
                _ => result,
            }
        }

        ResolvedStatement::While { span, condition, statement } => {
            let kind = condition.kind();
            let mut flag = true;
            while flag {
                flag = match (kind.clone(), evaluate(runtime, condition.clone())?) {
                    (ValueKind::Bool, Value::Bool(false)) | (ValueKind::Option(_), Value::None) => false,
                    (ValueKind::Bool, Value::Bool(true)) | (ValueKind::Option(_), _)  => true,
                    (_, other) => return Err(RuntimeError::Error{span, message:format!("While condition was not true or false: {:?}", other)}),
                };

                if flag { 
                    match execute_statement(runtime, *statement.clone())? {
                        ExecFlow::Normal(_) => {},
                        ExecFlow::Declare(_, local) => runtime.deref_local(local, 5),
                        ExecFlow::Break(_) => break,
                        ExecFlow::Continue(_) => continue,
                        ExecFlow::Return(span, value) => return Ok(ExecFlow::Return(span, value)),
                        ExecFlow::Error{span, message} => return Ok(ExecFlow::Error{span, message})
                    }
                }
            }
            Ok(ExecFlow::Normal(span))
        }

        ResolvedStatement::For{ span, local, target, statement } => {
            
            let iter: Box<dyn Iterator<Item = Value>> = match evaluate(runtime, target)? {
                Value::Set(val, _) => Box::new(val.into_iter()),
                Value::Array(vec, _) => Box::new(vec.into_iter()),
                Value::String(string) => Box::new(
                    string.into_bytes().into_iter()
                        .map(|c| Value::String((c as char).to_string()))
                ),
                Value::Range(start, end, by, inclusive, kind) => {
                    match kind {
                        NumKind::UInt8 | NumKind::UInt16 | NumKind::UInt32 | NumKind::UInt64 => {
                            let (start, end, by) = (start.to_u64().unwrap(), end.to_u64().unwrap(), by.to_u64().unwrap());
                            let range: Box<dyn Iterator<Item = u64>> = match inclusive {
                                true => Box::new((start..=end).step_by(by as usize)),
                                false => Box::new((start..end).step_by(by as usize)),
                            };
                            Box::new(range.map(|n| n.into()))
                        }
                        _ => {
                            let (start, end, by) = (start.to_i64().unwrap(), end.to_i64().unwrap(), by.to_i64().unwrap());
                            let range: Box<dyn Iterator<Item = i64>> = match (inclusive, start <= end) {
                                (true,  true)  => Box::new((start..=end).step_by(by as usize)),
                                (true,  false) => Box::new((0..=(start - end) / -by).map(move |i| start + i * by)),
                                (false, true)  => Box::new((start..end).step_by(by as usize)),
                                (false, false) => Box::new((0..(start - end) / -by).map(move |i| start + i * by)),
                            };
                            Box::new(range.map(|n| n.into()))
                        }
                    }
                    
                },
                other => return Err(RuntimeError::Error { span,
                    message: format!("{:?} is not iterable", other)
                }),
            };

            for item in iter {
                runtime.reserve_local(local, item);

                match execute_statement(runtime, *statement.clone())? {
                    ExecFlow::Continue(_) => continue,
                    ExecFlow::Break(_) => break,
                    ExecFlow::Normal(_) => {},
                    ExecFlow::Declare(_, local) => runtime.deref_local(local, 2),
                    flow => return Ok(flow)
                }

                runtime.deref_local(local, 1);
            }

            Ok(ExecFlow::Normal(span))
        }

        ResolvedStatement::Block(statements) => {
            let mut last_span = Span::default();
            let mut locals = Vec::new();

            for statement in statements{
                match execute_statement(runtime, statement)? {
                    ExecFlow::Normal(span) => last_span = span,
                    ExecFlow::Declare(_, local) => locals.push(local),
                    flow => { 
                        runtime.deref_locals(locals, 6);
                        return Ok(flow);
                    }
                }
            }
            runtime.deref_locals(locals, 7);
            Ok(ExecFlow::Normal(last_span))
        }

        ResolvedStatement::Break { span } => Ok(ExecFlow::Break(span)),
        ResolvedStatement::Continue { span } => Ok(ExecFlow::Continue(span)),
        ResolvedStatement::Return { span, value } => Ok(ExecFlow::Return(span, evaluate(runtime, value)?)),
        ResolvedStatement::Throw { span, message } => {
            match evaluate(runtime, message)? {
                Value::String(message) => Ok(ExecFlow::Error{span, message}),
                other => Err(RuntimeError::TypeMismatch{span, found: other, expected: ValueKind::String})
            }
        }

        other => return Err(RuntimeError::Error{span:Span::default(), message:format!("Invalid statement: {:?}", other)})
    }
}