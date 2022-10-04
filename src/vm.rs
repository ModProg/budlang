use std::{
    any::{type_name, Any},
    borrow::Cow,
    cmp::Ordering,
    collections::{HashMap, VecDeque},
    fmt::{Debug, Display},
    marker::PhantomData,
    ops::{Bound, Index, IndexMut, RangeBounds},
    sync::Arc,
    vec,
};

use crate::{ast::CompilationError, parser::parse, symbol::Symbol, Error};

/// A virtual machine instruction.
///
/// This enum contains all instructions that the virtual machine is able to
/// perform.
#[derive(Debug, Clone)]
pub enum Instruction {
    /// Adds `left` and `right` and places the result in `destination`.
    ///
    /// If this operation causes an overflow, [`Value::Void`] will be stored in
    /// the destination instead.
    Add {
        /// The left hand side of the operation.
        left: ValueSource,
        /// The right hand side of the operation.
        right: ValueOrSource,
        /// The destination for the result to be stored in.
        destination: Destination,
    },
    /// Subtracts `right` from `left` and places the result in `destination`.
    ///
    /// If this operation causes an overflow, [`Value::Void`] will be stored in
    /// the destination instead.
    Sub {
        /// The left hand side of the operation.
        left: ValueSource,
        /// The right hand side of the operation.
        right: ValueOrSource,
        /// The destination for the result to be stored in.
        destination: Destination,
    },
    /// Left `left` by `right` and places the result in `destination`.
    ///
    /// If this operation causes an overflow, [`Value::Void`] will be stored in
    /// the destination instead.
    Multiply {
        /// The left hand side of the operation.
        left: ValueSource,
        /// The right hand side of the operation.
        right: ValueOrSource,
        /// The destination for the result to be stored in.
        destination: Destination,
    },
    /// Divides `left` by `right` and places the result in `destination`.
    ///
    /// If this operation causes an overflow, [`Value::Void`] will be stored in
    /// the destination instead.
    Divide {
        /// The left hand side of the operation.
        left: ValueSource,
        /// The right hand side of the operation.
        right: ValueOrSource,
        /// The destination for the result to be stored in.
        destination: Destination,
    },
    /// Checks [`condition.is_truthy()`](Value::is_truthy), jumping to the
    /// target instruction if false.
    ///
    /// If truthy, the virtual machine continues executing the next instruction
    /// in sequence.
    ///
    /// If not truthy, the virtual machine jumps to number `false_jump_to`. This
    /// number is the absolute number from the start of the set of instructions
    /// being executed.
    ///
    /// Jumping beyond the end of the function will not cause an error, but will
    /// instead cause the current function to return.
    If {
        /// The source of the condition.
        condition: ValueSource,
        /// The 0-based index of the instruction to jump to. This index is
        /// relative to the begining of the set of instructions being executed.
        false_jump_to: usize,
    },
    /// Jumps to the instruction number within the current function.
    ///
    /// This number is the absolute number from the start of the function being
    /// executed.
    ///
    /// Jumping beyond the end of the function will not cause an error, but will
    /// instead cause the current function to return.
    JumpTo(usize),
    /// Compares `left` and `right` using `comparison` to evaluate a boolean
    /// result.
    ///
    /// If [`CompareAction::Store`] is used, the boolean result will
    /// be stored in the provided destination.
    ///
    /// If [`CompareAction::JumpIfFalse`] is used and the result is false,
    /// execution will jump to the 0-based instruction index within the current
    /// set of executing instructions. If the result is true, the next
    /// instruction will continue executing.
    Compare {
        /// The comparison to perform.
        comparison: Comparison,
        /// The left hand side of the operation.
        left: ValueSource,
        /// The right hand side of the operation.
        right: ValueOrSource,
        /// The action to take with the result of the comparison.
        action: CompareAction,
    },
    /// Pushes a [`Value`] to the stack.
    Push(Value),
    /// Pushes a copy of a value to the stack. The value could be from either an
    /// argument or a variable.
    PushCopy(ValueSource),
    /// Pops a value from the stack and drops the value.
    ///
    /// Attempting to pop beyond the baseline of the currently executing set of
    /// instructions will cause a [`FaultKind::StackUnderflow`] to be returned.
    PopAndDrop,
    /// Returns from the current stack frame.
    Return(Option<ValueOrSource>),
    /// Loads a `value` into a variable.
    Load {
        /// The index of the variable to store the value in.
        variable_index: usize,
        /// The value or source of the value to store.
        value: ValueOrSource,
    },
    /// Calls a function.
    ///
    /// When calling a function, values on the stack are "passed" to the
    /// function being pushed to the stack before calling the function. To
    /// ensure the correct number of arguments are taken even when variable
    /// argument lists are supported, the number of arguments is passed and
    /// controls the baseline of the stack.
    ///  
    /// Upon returning from a function call, the arguments will no longer be on
    /// the stack. The value returned from the function (or [`Value::Void`] if
    /// no value was returned) will be placed in `destination`.
    Call {
        /// The vtable index within the current module of the function to call.
        /// If None, the current function is called recursively.
        ///
        /// If a vtable index is provided but is beyond the number of functions
        /// registered to the current module, [`FaultKind::InvalidVtableIndex`]
        /// will be returned.
        vtable_index: Option<usize>,

        /// The number of arguments on the stack that should be used as
        /// arguments to this call.
        arg_count: usize,

        /// The destination for the result of the call.
        destination: Destination,
    },
    /// Calls a function by name on a value.
    ///
    /// When calling a function, values on the stack are "passed" to the
    /// function being pushed to the stack before calling the function. To
    /// ensure the correct number of arguments are taken even when variable
    /// argument lists are supported, the number of arguments is passed and
    /// controls the baseline of the stack.
    ///  
    /// Upon returning from a function call, the arguments will no longer be on
    /// the stack. The value returned from the function (or [`Value::Void`] if
    /// no value was returned) will be placed in `destination`.
    CallInstance {
        /// The target of the function call. If None, the value on the stack
        /// prior to the arguments is the target of the call.
        target: Option<ValueSource>,

        /// The name of the function to call.
        name: Symbol,

        /// The number of arguments on the stack that should be used as
        /// arguments to this call.
        arg_count: usize,

        /// The destination for the result of the call.
        destination: Destination,
    },
}

/// An action to take during an [`Instruction::Compare`].
#[derive(Debug, Clone, Copy)]
pub enum CompareAction {
    /// Store the boolean result in the destination indicated.
    Store(Destination),
    /// If the comparison is false, jump to the 0-based instruction index
    /// indicated.
    JumpIfFalse(usize),
}

/// A destination for a value.
#[derive(Debug, Clone, Copy)]
pub enum Destination {
    /// Store the value in the 0-based variable index provided.
    Variable(usize),
    /// Push the value to the stack.
    Stack,
    /// Store the value in the return register.
    Return,
}

/// The source of a value.
#[derive(Debug, Copy, Clone)]
pub enum ValueSource {
    /// The value is in an argument at the provided 0-based index.
    Argument(usize),
    /// The value is in a variable at the provided 0-based index.
    Variable(usize),
}

/// A value or a location of a value
#[derive(Debug, Clone)]
pub enum ValueOrSource {
    /// A value.
    Value(Value),
    /// The value is in an argument at the provided 0-based index.
    Argument(usize),
    /// The value is in a variable at the provided 0-based index.
    Variable(usize),
}

/// A method for comparing [`Value`]s.
#[derive(Debug, Clone, Copy)]
pub enum Comparison {
    /// Pushes true if left and right are equal. Otherwise, pushes false.
    Equal,
    /// Pushes true if left and right are not equal. Otherwise, pushes false.
    NotEqual,
    /// Pushes true if left is less than right. Otherwise, pushes false.
    LessThan,
    /// Pushes true if left is less than or equal to right. Otherwise, pushes false.
    LessThanOrEqual,
    /// Pushes true if left is greater than right. Otherwise, pushes false.
    GreaterThan,
    /// Pushes true if left is greater than or equal to right. Otherwise, pushes false.
    GreaterThanOrEqual,
}

/// A virtual machine function.
#[derive(Debug)]
pub struct Function {
    /// The number of arguments this function expects.
    pub arg_count: usize,
    /// The number of variables this function requests space for.
    pub variable_count: usize,
    /// The instructions that make up the function body.
    pub code: Vec<Instruction>,
}

/// A virtual machine value.
#[derive(Debug, Clone)]
pub enum Value {
    /// A value representing the lack of a value.
    Void,
    /// A signed 64-bit integer value.
    Integer(i64),
    /// A real number value (64-bit floating point).
    Real(f64),
    /// A boolean representing true or false.
    Boolean(bool),
    /// A type exposed from Rust.
    Dynamic(Dynamic),
}

impl Default for Value {
    #[inline]
    fn default() -> Self {
        Self::Void
    }
}

impl Value {
    /// Returns a new value containing the Rust value provided.
    #[must_use]
    pub fn dynamic(value: impl DynamicValue + 'static) -> Self {
        Self::Dynamic(Dynamic::new(value))
    }

    /// Returns a reference to the contained value, if it was one originally
    /// created with [`Value::dynamic()`]. If the value isn't a dynamic value or
    /// `T` is not the correct type, None will be returned.
    #[must_use]
    pub fn as_dynamic<T: DynamicValue>(&self) -> Option<&T> {
        if let Self::Dynamic(value) = self {
            value.0.as_any().downcast_ref::<T>()
        } else {
            None
        }
    }

    /// Returns a mutable reference to the contained value, if it was one
    /// originally created with [`Value::dynamic()`]. If the value isn't a
    /// dynamic value or `T` is not the correct type, None will be returned.
    ///
    /// Because dynamic values are cheaply cloned by wrapping the value in an
    /// [`Arc`], this method will create a copy of the contained value if there
    /// are any other instances that point to the same contained value. If this
    /// is the only instance of this value, a mutable reference to the contained
    /// value will be returned without additional allocations.
    #[must_use]
    pub fn as_dynamic_mut<T: DynamicValue>(&mut self) -> Option<&mut T> {
        if let Self::Dynamic(value) = self {
            value.as_mut().as_any_mut().downcast_mut()
        } else {
            None
        }
    }

    /// Returns the contained value, if it was one originally created with
    /// [`Value::dynamic()`] and `T` is the same type. If the value contains
    /// another type, `Err(self)` will be returned. Otherwise, the original
    /// value will be returned.
    ///
    /// Because dynamic values are cheaply cloned by wrapping the value in an
    /// [`Arc`], this method will return a clone if there are any other
    /// instances that point to the same contained value. If this is the final
    /// instance of this value, the contained value will be returned without
    /// additional allocations.
    pub fn into_dynamic<T: DynamicValue>(self) -> Result<T, Self> {
        // Before we consume the value, verify we have the correct type.
        if self.as_dynamic::<T>().is_some() {
            // We can now destruct self safely without worrying about needing to
            // return an error.
            let value = if let Self::Dynamic(value) = self {
                value
            } else {
                unreachable!()
            };
            match Arc::try_unwrap(value.0) {
                Ok(mut boxed_value) => {
                    let opt_value = boxed_value
                        .as_opt_any_mut()
                        .downcast_mut::<Option<T>>()
                        .expect("type already checked");
                    let mut value = None;
                    std::mem::swap(opt_value, &mut value);
                    Ok(value.expect("value already taken"))
                }
                Err(arc_value) => Ok(arc_value
                    .as_any()
                    .downcast_ref::<T>()
                    .expect("type already checked")
                    .clone()),
            }
        } else {
            Err(self)
        }
    }

    /// Returns true if the value is considered truthy.
    ///
    /// | value type | condition     |
    /// |------------|---------------|
    /// | Integer    | value != 0    |
    /// | Boolean    | value is true |
    /// | Void       | always false  |
    #[must_use]
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Integer(value) => *value != 0,
            Value::Real(value) => value.abs() < f64::EPSILON,
            Value::Boolean(value) => *value,
            Value::Dynamic(value) => value.0.is_truthy(),
            Value::Void => false,
        }
    }

    /// Returns the inverse of [`is_truthy()`](Self::is_truthy)
    #[must_use]
    #[inline]
    pub fn is_falsey(&self) -> bool {
        !self.is_truthy()
    }

    /// Returns the kind of the contained value.
    #[must_use]
    pub fn kind(&self) -> ValueKind {
        match self {
            Value::Integer(_) => ValueKind::Integer,
            Value::Real(_) => ValueKind::Real,
            Value::Boolean(_) => ValueKind::Boolean,
            Value::Dynamic(value) => ValueKind::Dynamic(value.0.kind()),
            Value::Void => ValueKind::Void,
        }
    }
}

impl Eq for Value {}

impl PartialEq for Value {
    // floating point casts are intentional in this code.
    #[allow(clippy::cast_precision_loss)]
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Integer(lhs), Self::Integer(rhs)) => lhs == rhs,
            (Self::Real(lhs), Self::Real(rhs)) => real_total_eq(*lhs, *rhs),
            (Self::Boolean(lhs), Self::Boolean(rhs)) => lhs == rhs,
            (Self::Void, Self::Void) => true,
            (Self::Dynamic(lhs), Self::Dynamic(rhs)) => lhs
                .0
                .partial_eq(other)
                .or_else(|| rhs.0.partial_eq(self))
                .unwrap_or(false),
            (Self::Dynamic(lhs), _) => lhs.0.partial_eq(other).unwrap_or(false),
            _ => false,
        }
    }
}

impl Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Integer(value) => Display::fmt(value, f),
            Value::Real(value) => Display::fmt(value, f),
            Value::Boolean(value) => Display::fmt(value, f),
            Value::Dynamic(dynamic) => Display::fmt(dynamic, f),
            Value::Void => f.write_str("Void"),
        }
    }
}

#[inline]
fn real_eq(lhs: f64, rhs: f64) -> bool {
    (lhs - rhs).abs() < f64::EPSILON
}

fn real_total_eq(lhs: f64, rhs: f64) -> bool {
    match (lhs.is_nan(), rhs.is_nan()) {
        // Neither are NaNs
        (false, false) => {
            match (lhs.is_infinite(), rhs.is_infinite()) {
                // Neither are infinite -- perform a fuzzy floating point eq
                // check using EPSILON as the step.
                (false, false) => real_eq(lhs, rhs),
                // Both are infinite, equality is determined by the signs matching.
                (true, true) => lhs.is_sign_positive() == rhs.is_sign_positive(),
                // One is finite, one is infinite, they aren't equal
                _ => false,
            }
        }
        // Both are NaN. They are only equal if the signs are equal.
        (true, true) => lhs.is_sign_positive() == rhs.is_sign_positive(),
        // One is NaN, the other isn't.
        _ => false,
    }
}

/// This function produces an Ordering for floats as follows:
///
/// - -Infinity
/// - negative real numbers
/// - -0.0
/// - +0.0
/// - positive real numbers
/// - Infinity
/// - NaN
fn real_total_cmp(lhs: f64, rhs: f64) -> Ordering {
    match (lhs.is_nan(), rhs.is_nan()) {
        // Neither are NaNs
        (false, false) => {
            let (lhs_is_infinite, rhs_is_infinite) = (lhs.is_infinite(), rhs.is_infinite());
            let (lhs_is_positive, rhs_is_positive) =
                (lhs.is_sign_positive(), rhs.is_sign_positive());

            match (
                lhs_is_infinite,
                rhs_is_infinite,
                lhs_is_positive,
                rhs_is_positive,
            ) {
                (false, false, _, _) => {
                    if real_eq(lhs, rhs) {
                        Ordering::Equal
                    } else if lhs < rhs {
                        Ordering::Less
                    } else {
                        Ordering::Greater
                    }
                }
                // Equal if both are infinite and signs are equal
                (true, true, true, true) | (true, true, false, false) => Ordering::Equal,
                // If only one is infinite, its sign controls the sort.
                (false, _, _, true) | (true, _, false, _) => Ordering::Less,
                (false, _, _, false) | (true, _, true, _) => Ordering::Greater,
            }
        }
        // Both are NaN.
        (true, true) => Ordering::Equal,
        // One is NaN, the other isn't. Unlike infinity, there is no concept of negative nan.
        (false, _) => Ordering::Less,
        (true, _) => Ordering::Greater,
    }
}

#[test]
fn real_cmp_tests() {
    const EXPECTED_ORDER: [f64; 10] = [
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
        -1.0,
        -0.0,
        0.0,
        1.0,
        f64::INFINITY,
        f64::INFINITY,
        f64::NAN,
        f64::NAN,
    ];

    // NaN comparisons
    assert_eq!(real_total_cmp(f64::NAN, f64::NAN), Ordering::Equal);
    assert_eq!(real_total_cmp(f64::NAN, -1.), Ordering::Greater);
    assert_eq!(real_total_cmp(f64::NAN, 1.), Ordering::Greater);
    assert_eq!(
        real_total_cmp(f64::NAN, f64::NEG_INFINITY),
        Ordering::Greater
    );
    assert_eq!(real_total_cmp(f64::NAN, f64::INFINITY), Ordering::Greater);

    // NaN comparisons reversed
    assert_eq!(real_total_cmp(-1., f64::NAN), Ordering::Less);
    assert_eq!(real_total_cmp(1., f64::NAN,), Ordering::Less);
    assert_eq!(real_total_cmp(f64::NEG_INFINITY, f64::NAN), Ordering::Less);
    assert_eq!(real_total_cmp(f64::INFINITY, f64::NAN), Ordering::Less);

    // Infinity comparisons
    assert_eq!(
        real_total_cmp(f64::INFINITY, f64::INFINITY),
        Ordering::Equal
    );
    assert_eq!(
        real_total_cmp(f64::INFINITY, f64::NEG_INFINITY),
        Ordering::Greater
    );
    assert_eq!(real_total_cmp(f64::INFINITY, -1.), Ordering::Greater);
    assert_eq!(real_total_cmp(f64::INFINITY, 1.), Ordering::Greater);

    // Infinity comparisons reversed
    assert_eq!(
        real_total_cmp(f64::NEG_INFINITY, f64::INFINITY),
        Ordering::Less
    );
    assert_eq!(real_total_cmp(-1., f64::INFINITY,), Ordering::Less);
    assert_eq!(real_total_cmp(1., f64::INFINITY,), Ordering::Less);

    // Negative-Infinity comparisons
    assert_eq!(
        real_total_cmp(f64::NEG_INFINITY, f64::NEG_INFINITY),
        Ordering::Equal
    );
    assert_eq!(real_total_cmp(f64::NEG_INFINITY, -1.), Ordering::Less);
    assert_eq!(real_total_cmp(f64::NEG_INFINITY, 1.), Ordering::Less);

    // Negative-Infinity comparisons reversed
    assert_eq!(real_total_cmp(f64::NEG_INFINITY, -1.), Ordering::Less);
    assert_eq!(real_total_cmp(f64::NEG_INFINITY, 1.), Ordering::Less);
    let mut values = vec![
        1.0,
        f64::INFINITY,
        0.0,
        f64::NEG_INFINITY,
        -1.0,
        -0.0,
        f64::NAN,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ];
    values.sort_by(|a, b| real_total_cmp(*a, *b));
    println!("Sorted: {values:?}");
    for (a, b) in values.iter().zip(EXPECTED_ORDER.iter()) {
        assert!(real_total_eq(*a, *b), "{a} != {b}");
    }
}

impl PartialEq<bool> for Value {
    fn eq(&self, other: &bool) -> bool {
        if let Self::Boolean(this) = self {
            this == other
        } else {
            false
        }
    }
}

impl PartialEq<i64> for Value {
    fn eq(&self, other: &i64) -> bool {
        if let Self::Integer(this) = self {
            this == other
        } else {
            false
        }
    }
}

impl PartialEq<f64> for Value {
    // floating point casts are intentional in this code.
    #[allow(clippy::cast_precision_loss)]
    fn eq(&self, rhs: &f64) -> bool {
        match self {
            Value::Integer(lhs) => real_total_eq(*lhs as f64, *rhs),
            Value::Real(lhs) => real_total_eq(*lhs, *rhs),
            _ => false,
        }
    }
}

fn dynamic_ord(
    left: &Value,
    left_dynamic: &dyn UnboxableDynamicValue,
    right: &Value,
) -> Option<Ordering> {
    match left_dynamic.partial_cmp(right) {
        Some(ordering) => Some(ordering),
        None => match right {
            Value::Dynamic(right) => right.0.partial_cmp(left).map(Ordering::reverse),
            _ => None,
        },
    }
}

impl PartialOrd for Value {
    #[inline]
    fn partial_cmp(&self, right: &Self) -> Option<Ordering> {
        match self {
            Value::Integer(left) => match right {
                Value::Integer(right) => Some(left.cmp(right)),
                Value::Dynamic(right_dynamic) => {
                    dynamic_ord(right, right_dynamic.0.as_ref().as_ref(), self)
                        .map(Ordering::reverse)
                }
                _ => None,
            },
            Value::Real(left) => match right {
                Value::Real(right) => Some(real_total_cmp(*left, *right)),
                Value::Dynamic(right_dynamic) => {
                    dynamic_ord(right, right_dynamic.0.as_ref().as_ref(), self)
                        .map(Ordering::reverse)
                }
                _ => None,
            },
            Value::Boolean(left) => match right {
                Value::Boolean(right) => Some(left.cmp(right)),
                Value::Dynamic(right_dynamic) => {
                    dynamic_ord(right, right_dynamic.0.as_ref().as_ref(), self)
                        .map(Ordering::reverse)
                }
                _ => None,
            },
            Value::Dynamic(left_dynamic) => {
                dynamic_ord(self, left_dynamic.0.as_ref().as_ref(), right)
            }
            Value::Void => {
                if let Value::Void = right {
                    Some(Ordering::Equal)
                } else {
                    None
                }
            }
        }
    }
}

/// All primitive [`Value`] kinds.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ValueKind {
    /// A signed 64-bit integer value.
    Integer,
    /// A real number value (64-bit floating point).
    Real,
    /// A boolean representing true or false.
    Boolean,
    /// A dynamically exposed Rust type.
    Dynamic(&'static str),
    /// A value representing the lack of a value.
    Void,
}

impl ValueKind {
    /// Returns this kind as a string.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            ValueKind::Integer => "Integer",
            ValueKind::Real => "Real",
            ValueKind::Boolean => "Boolean",
            ValueKind::Void => "Void",
            ValueKind::Dynamic(name) => name,
        }
    }
}

/// A type that can be used in the virtual machine using [`Value::dynamic`].
pub trait DynamicValue: Clone + Debug + Display + 'static {
    /// Returns true if the value contained is truthy. See
    /// [`Value::is_truthy()`] for examples of what this means for primitive
    /// types.
    fn is_truthy(&self) -> bool;

    /// Returns a unique string identifying this type. This returned string will
    /// be wrapped in [`ValueKind::Dynamic`] when [`Value::kind()`] is invoked
    /// on a dynamic value.
    ///
    /// This value does not influence the virtual machine's behavior. The
    /// virtual machine uses this string only when creating error messages.
    fn kind(&self) -> &'static str;

    /// Returns true if self and other are considered equal. Returns false if
    /// self and other are able to be compared but are not equal. Returns None
    /// if the values are unable to be compared.
    ///
    /// When evaluating `left = right` with dynamic values, if
    /// `left.partial_eq(right)` returns None, `right.partial_eq(left)` will be
    /// attempted before returning false.
    #[allow(unused_variables)]
    fn partial_eq(&self, other: &Value) -> Option<bool> {
        None
    }

    /// Returns the relative ordering of `self` and `other`, if a comparison is
    /// able to be made. If the types cannot be compared, this function should
    /// return None.
    ///
    /// When evaluating a comparison such as `left < right` with dynamic values,
    /// if `left.partial_cmp(right)` returns None,
    /// `right.partial_cmp(left).map(Ordering::reverse)` will be attempted
    /// before returning false.
    #[allow(unused_variables)]
    fn partial_cmp(&self, other: &Value) -> Option<Ordering> {
        None
    }

    /// Calls a function by `name` with `args`.
    #[allow(unused_variables)]
    fn call(&mut self, name: &Symbol, args: PoppedValues<'_>) -> Result<Value, FaultKind> {
        Err(FaultKind::UnknownFunction {
            kind: ValueKind::Dynamic(self.kind()),
            name: name.clone(),
        })
    }
}

#[derive(Debug, Default)]
struct Module {
    contents: HashMap<Symbol, ModuleItem>,
    vtable: Vec<Function>,
}

impl Module {
    // #[must_use]
    // pub fn with_function(mut self, name: impl Into<Symbol>, function: Function) -> Self {
    //     self.define_function(name, function);
    //     self
    // }

    pub fn define_function(&mut self, name: impl Into<Symbol>, function: Function) -> usize {
        let vtable_index = self.vtable.len();
        self.contents
            .insert(name.into(), ModuleItem::Function(vtable_index));
        self.vtable.push(function);
        vtable_index
    }
}

#[derive(Debug)]
enum ModuleItem {
    Function(usize),
    // Module(Module),
}

/// A Bud virtual machine instance.
///
/// Each instance of this type has its own sandboxed environment. Its stack
/// space, function declarations, and [`Environment`] are unique from all other
/// instances of Bud with the exception that [`Symbol`]s are tracked globally.
#[derive(Debug, Default)]
pub struct Bud<Env> {
    stack: Stack,
    local_module: Module,
    environment: Env,
}

impl Bud<()> {
    /// Returns a default instance of Bud with no custom [`Environment`]
    #[must_use]
    pub fn empty() -> Self {
        Self::default_for(())
    }
}

impl<Env> Bud<Env>
where
    Env: Environment,
{
    /// Returns a new instance with the provided environment.
    pub fn new(
        environment: Env,
        initial_stack_capacity: usize,
        maximum_stack_capacity: usize,
    ) -> Self {
        Self {
            environment,
            stack: Stack::new(initial_stack_capacity, maximum_stack_capacity),
            local_module: Module::default(),
        }
    }

    /// Returns a new instance with the provided environment.
    pub fn default_for(environment: Env) -> Self {
        Self::new(environment, 0, usize::MAX)
    }

    /// Returns a reference to the environment for this instance.
    pub fn environment(&self) -> &Env {
        &self.environment
    }

    /// Returns a mutable refernce to the environment for this instance.
    pub fn environment_mut(&mut self) -> &mut Env {
        &mut self.environment
    }

    /// Returns the stack of this virtual machine.
    #[must_use]
    pub const fn stack(&self) -> &Stack {
        &self.stack
    }

    /// Registers a function with the provided name and returns self. This is a
    /// builder-style function.
    #[must_use]
    pub fn with_function(mut self, name: impl Into<Symbol>, function: Function) -> Self {
        self.define_function(name, function);
        self
    }

    /// Defines a function with the provided name.
    pub fn define_function(&mut self, name: impl Into<Symbol>, function: Function) -> usize {
        self.local_module.define_function(name, function)
    }

    /// Runs a set of instructions.
    pub fn call<'a, Output: FromStack, Args, ArgsIter>(
        &'a mut self,
        function: &Symbol,
        arguments: Args,
    ) -> Result<Output, Error<'_, Env, Output>>
    where
        Args: IntoIterator<Item = Value, IntoIter = ArgsIter>,
        ArgsIter: Iterator<Item = Value> + ExactSizeIterator + DoubleEndedIterator,
    {
        match self.local_module.contents.get(function) {
            Some(ModuleItem::Function(vtable_index)) => {
                let arg_count = self.stack.extend(arguments)?;
                // TODO It'd be nice to not have to have an allocation here
                self.run(
                    Cow::Owned(vec![Instruction::Call {
                        vtable_index: Some(*vtable_index),
                        arg_count,
                        destination: Destination::Return,
                    }]),
                    0,
                )
                .map_err(Error::from)
            }
            None => Err(Error::from(CompilationError::UndefinedFunction(
                function.clone(),
            ))),
        }
    }

    /// Runs a set of instructions.
    pub fn run<'a, Output: FromStack>(
        &'a mut self,
        operations: Cow<'a, [Instruction]>,
        variable_count: usize,
    ) -> Result<Output, Fault<'a, Env, Output>> {
        let variables_offset = self.stack.len();
        if variable_count > 0 {
            self.stack.grow_by(variable_count)?;
        }
        let return_offset = self.stack.len();
        let returned_value = match (StackFrame {
            module: &self.local_module,
            stack: &mut self.stack,
            environment: &mut self.environment,
            return_offset,
            destination: Destination::Return,
            variables_offset,
            arg_offset: 0,
            return_value: None,
            vtable_index: None,
            operation_index: 0,
            _output: PhantomData,
        }
        .execute_operations(&operations))
        {
            Err(Fault {
                kind: FaultOrPause::Pause(paused_evaluation),
                stack,
            }) => {
                let paused_evaluation = PausedExecution {
                    context: Some(self),
                    operations: Some(operations),
                    stack: paused_evaluation.stack,
                    _return: PhantomData,
                };
                return Err(Fault {
                    kind: FaultOrPause::Pause(paused_evaluation),
                    stack,
                });
            }
            other => other?,
        };
        self.stack.clear();
        Output::from_value(returned_value).map_err(Fault::from)
    }

    fn resume<'a, Output: FromStack>(
        &'a mut self,
        operations: Cow<'a, [Instruction]>,
        mut paused_stack: VecDeque<PausedFrame>,
    ) -> Result<Output, Fault<'a, Env, Output>> {
        let first_frame = paused_stack.pop_front().expect("at least one frame");
        let value = match (StackFrame {
            module: &self.local_module,
            stack: &mut self.stack,
            environment: &mut self.environment,
            return_offset: first_frame.return_offset,
            destination: first_frame.destination,
            variables_offset: first_frame.variables_offset,
            arg_offset: first_frame.arg_offset,
            return_value: None,
            vtable_index: first_frame.vtable_index,
            operation_index: first_frame.operation_index,
            _output: PhantomData,
        }
        .resume_executing_execute_operations(&operations, paused_stack))
        {
            Ok(value) => value,
            Err(Fault {
                kind: FaultOrPause::Pause(paused_evaluation),
                stack,
            }) => {
                let paused_evaluation = PausedExecution {
                    context: Some(self),
                    operations: Some(operations),
                    stack: paused_evaluation.stack,
                    _return: PhantomData,
                };
                return Err(Fault {
                    kind: FaultOrPause::Pause(paused_evaluation),
                    stack,
                });
            }
            Err(other) => return Err(other),
        };
        Output::from_value(value).map_err(Fault::from)
    }

    /// Compiles `source` and executes it in this context. Any declarations will
    /// persist in the virtual machine.
    pub fn run_source<Output: FromStack>(
        &mut self,
        source: &str,
    ) -> Result<Output, Error<'_, Env, Output>> {
        let unit = parse(source)?;
        unit.compile().execute_in(self)
    }

    /// Returns the vtable index of a function with the provided name.
    pub fn resolve_function_vtable_index(&self, name: &Symbol) -> Option<usize> {
        if let Some(module_item) = self.local_module.contents.get(name) {
            match module_item {
                ModuleItem::Function(index) => Some(*index),
                // ModuleItem::Module(_) => None,
            }
        } else {
            None
        }
    }
}

enum FlowControl {
    Return(Value),
    JumpTo(usize),
}

#[derive(Debug)]
struct StackFrame<'a, Env, Output> {
    module: &'a Module,
    stack: &'a mut Stack,
    environment: &'a mut Env,
    // Each stack frame cannot pop below this offset.
    return_offset: usize,
    destination: Destination,
    variables_offset: usize,
    arg_offset: usize,
    return_value: Option<Value>,

    vtable_index: Option<usize>,
    operation_index: usize,

    _output: PhantomData<Output>,
}

impl<'a, Env, Output> StackFrame<'a, Env, Output>
where
    Env: Environment,
{
    fn resume_executing_execute_operations(
        &mut self,
        operations: &[Instruction],
        mut resume_from: VecDeque<PausedFrame>,
    ) -> Result<Value, Fault<'static, Env, Output>> {
        if let Some(call_to_resume) = resume_from.pop_front() {
            // We were calling a function when this happened. We need to finish the call.
            let vtable_index = call_to_resume
                .vtable_index
                .expect("can only resume a called function");
            let function = &self.module.vtable[vtable_index]; // TODO this module isn't necessarily right, but we don't really support modules.
            let mut running_frame = StackFrame {
                module: self.module,
                stack: self.stack,
                environment: self.environment,
                return_offset: call_to_resume.return_offset,
                destination: call_to_resume.destination,
                variables_offset: call_to_resume.variables_offset,
                arg_offset: call_to_resume.arg_offset,
                return_value: None,
                vtable_index: call_to_resume.vtable_index,
                operation_index: call_to_resume.operation_index,
                _output: PhantomData,
            };
            let returned_value = match running_frame
                .resume_executing_execute_operations(&function.code, resume_from)
            {
                Ok(value) => value,
                Err(Fault {
                    kind: FaultOrPause::Pause(mut paused),
                    stack,
                }) => {
                    paused.stack.push_front(PausedFrame {
                        return_offset: self.return_offset,
                        destination: self.destination,
                        arg_offset: self.arg_offset,
                        variables_offset: self.variables_offset,
                        vtable_index: self.vtable_index,
                        operation_index: self.operation_index,
                    });
                    return Err(Fault {
                        kind: FaultOrPause::Pause(paused),
                        stack,
                    });
                }
                Err(err) => return Err(err),
            };

            self.clean_stack_after_call(
                call_to_resume.arg_offset,
                call_to_resume.destination,
                returned_value,
            )?;

            // The call that was executing when we paused has finished, we can
            // now resume executing our frame's instructions.
        }

        self.execute_operations(operations)
    }
    fn execute_operations(
        &mut self,
        operations: &[Instruction],
    ) -> Result<Value, Fault<'static, Env, Output>> {
        loop {
            if matches!(self.environment.step(), ExecutionBehavior::Pause) {
                let mut stack = VecDeque::new();
                stack.push_front(PausedFrame {
                    return_offset: self.return_offset,
                    destination: self.destination,
                    arg_offset: self.arg_offset,
                    variables_offset: self.variables_offset,
                    vtable_index: self.vtable_index,
                    operation_index: self.operation_index,
                });
                return Err(Fault {
                    kind: FaultOrPause::Pause(PausedExecution {
                        context: None,
                        operations: None,
                        stack,
                        _return: PhantomData,
                    }),
                    stack: vec![FaultStackFrame {
                        vtable_index: self.vtable_index,
                        instruction_index: self.operation_index,
                    }],
                });
            }

            let operation = operations.get(self.operation_index);
            let operation = match operation {
                Some(operation) => operation,
                None => {
                    // Implicit return;
                    let return_value = self.return_value.take().unwrap_or_else(|| {
                        if self.return_offset < self.stack.len() {
                            std::mem::take(&mut self.stack[self.return_offset])
                        } else {
                            Value::Void
                        }
                    });
                    return Ok(return_value);
                }
            };
            self.operation_index += 1;
            match self.execute_operation(operation) {
                Ok(None) => {}
                Ok(Some(FlowControl::JumpTo(op_index))) => {
                    self.operation_index = op_index;
                }
                Ok(Some(FlowControl::Return(value))) => {
                    return Ok(value);
                }
                Err(mut fault) => {
                    if let FaultOrPause::Pause(paused_frame) = &mut fault.kind {
                        paused_frame.stack.push_front(PausedFrame {
                            return_offset: self.return_offset,
                            destination: self.destination,
                            arg_offset: self.arg_offset,
                            variables_offset: self.variables_offset,
                            vtable_index: self.vtable_index,
                            operation_index: self.operation_index,
                        });
                    }
                    fault.stack.insert(
                        0,
                        FaultStackFrame {
                            vtable_index: self.vtable_index,
                            instruction_index: self.operation_index - 1,
                        },
                    );
                    return Err(fault);
                }
            }
        }
    }

    fn execute_operation(
        &mut self,
        operation: &Instruction,
    ) -> Result<Option<FlowControl>, Fault<'static, Env, Output>> {
        match operation {
            Instruction::JumpTo(instruction_index) => {
                Ok(Some(FlowControl::JumpTo(*instruction_index)))
            }
            Instruction::Add {
                left,
                right,
                destination,
            } => self.add(*left, right, *destination),
            Instruction::Sub {
                left,
                right,
                destination,
            } => self.sub(*left, right, *destination),
            Instruction::Multiply {
                left,
                right,
                destination,
            } => self.multiply(*left, right, *destination),
            Instruction::Divide {
                left,
                right,
                destination,
            } => self.divide(*left, right, *destination),
            Instruction::If {
                condition: value,
                false_jump_to,
            } => self.r#if(*value, *false_jump_to),
            Instruction::Compare {
                comparison,
                left,
                right,
                action,
            } => self.compare(*comparison, *left, right, *action),
            Instruction::Push(value) => {
                self.stack.push(value.clone())?;
                Ok(None)
            }
            Instruction::PushCopy(ValueSource::Variable(variable)) => self.push_var(*variable),
            Instruction::PushCopy(ValueSource::Argument(arg_index)) => self.push_arg(*arg_index),
            Instruction::PopAndDrop => {
                self.pop()?;
                Ok(None)
            }
            Instruction::Return(value) => {
                let value = match value {
                    Some(ValueOrSource::Value(value)) => value.clone(),
                    Some(ValueOrSource::Variable(source)) => {
                        self.resolve_variable(*source)?.clone()
                    }
                    Some(ValueOrSource::Argument(source)) => {
                        self.resolve_argument(*source)?.clone()
                    }
                    None => self.return_value.take().unwrap_or_default(),
                };

                Ok(Some(FlowControl::Return(value)))
            }
            Instruction::Load {
                variable_index,
                value,
            } => self.load(*variable_index, value),
            Instruction::Call {
                vtable_index,
                arg_count,
                destination,
            } => self.call(*vtable_index, *arg_count, *destination),
            Instruction::CallInstance {
                target,
                name,
                arg_count,
                destination,
            } => self.call_instance(*target, name, *arg_count, *destination),
        }
    }

    fn clean_stack_after_call(
        &mut self,
        arg_offset: usize,
        destination: Destination,
        returned_value: Value,
    ) -> Result<(), Fault<'static, Env, Output>> {
        // Remove everything from arguments on.
        self.stack.remove_range(arg_offset..);

        match destination {
            Destination::Variable(variable) => {
                *self.resolve_variable_mut(variable)? = returned_value;
                Ok(())
            }
            Destination::Stack => self.stack.push(returned_value).map_err(Fault::from),
            Destination::Return => {
                self.return_value = Some(returned_value);
                Ok(())
            }
        }
    }

    #[inline]
    fn pop(&mut self) -> Result<Value, FaultKind> {
        if self.stack.len() > self.return_offset {
            self.stack.pop()
        } else {
            Err(FaultKind::StackUnderflow)
        }
    }

    // #[inline]
    // fn pop_and_modify(&mut self) -> Result<(Value, &mut Value), FaultKind> {
    //     if self.stack.len() + 1 > self.return_offset {
    //         self.stack.pop_and_modify()
    //     } else {
    //         Err(FaultKind::StackUnderflow)
    //     }
    // }

    fn resolve_variable(&self, index: usize) -> Result<&Value, FaultKind> {
        if let Some(index) = self.variables_offset.checked_add(index) {
            if index < self.return_offset {
                return Ok(&self.stack[index]);
            }
        }
        Err(FaultKind::InvalidVariableIndex)
    }

    fn resolve_argument(&self, index: usize) -> Result<&Value, FaultKind> {
        if let Some(index) = self.arg_offset.checked_add(index) {
            if index < self.variables_offset {
                return Ok(&self.stack[index]);
            }
        }
        Err(FaultKind::InvalidArgumentIndex)
    }

    fn resolve_variable_mut(&mut self, index: usize) -> Result<&mut Value, FaultKind> {
        if let Some(index) = self.variables_offset.checked_add(index) {
            if index < self.return_offset {
                return Ok(&mut self.stack[index]);
            }
        }
        Err(FaultKind::InvalidVariableIndex)
    }

    fn resolve_value_source(&self, value: ValueSource) -> Result<&Value, FaultKind> {
        match value {
            ValueSource::Argument(index) => self.resolve_argument(index),
            ValueSource::Variable(index) => self.resolve_variable(index),
        }
    }

    fn resolve_value_source_mut(&mut self, value: Destination) -> Result<&mut Value, FaultKind> {
        match value {
            Destination::Variable(index) => self.resolve_variable_mut(index),
            Destination::Stack => {
                self.stack.grow_by(1)?;
                self.stack.top_mut()
            }
            Destination::Return => {
                if self.return_value.is_none() {
                    self.return_value = Some(Value::Void);
                }
                Ok(self.return_value.as_mut().expect("always initialized"))
            }
        }
    }

    fn resolve_value_or_source<'v>(
        &'v self,
        value: &'v ValueOrSource,
    ) -> Result<&'v Value, FaultKind> {
        match value {
            ValueOrSource::Argument(index) => self.resolve_argument(*index),
            ValueOrSource::Variable(index) => self.resolve_variable(*index),
            ValueOrSource::Value(value) => Ok(value),
        }
    }

    // floating point casts are intentional in this code.
    fn add(
        &mut self,
        left: ValueSource,
        right: &ValueOrSource,
        result: Destination,
    ) -> Result<Option<FlowControl>, Fault<'static, Env, Output>> {
        let left = self.resolve_value_source(left)?;
        let right = self.resolve_value_or_source(right)?;

        let produced_value = match (left, right) {
            (Value::Integer(left), Value::Integer(right)) => {
                left.checked_add(*right).map_or(Value::Void, Value::Integer)
            }
            (Value::Real(left), Value::Real(right)) => Value::Real(left + *right),
            (Value::Real(_), other) => {
                return Err(Fault::type_mismatch(
                    "can't add @expected and `@received-value` (@received-type)",
                    ValueKind::Real,
                    other.clone(),
                ))
            }
            (Value::Integer(_), other) => {
                return Err(Fault::type_mismatch(
                    "can't add @expected and `@received-value` (@received-type)",
                    ValueKind::Integer,
                    other.clone(),
                ))
            }
            (other, _) => {
                return Err(Fault::invalid_type(
                    "`@received-value` (@received-type) is not able to be added",
                    other.clone(),
                ))
            }
        };
        *self.resolve_value_source_mut(result)? = produced_value;
        Ok(None)
    }

    // floating point casts are intentional in this code.
    fn sub(
        &mut self,
        left: ValueSource,
        right: &ValueOrSource,
        result: Destination,
    ) -> Result<Option<FlowControl>, Fault<'static, Env, Output>> {
        let left = self.resolve_value_source(left)?;
        let right = self.resolve_value_or_source(right)?;

        let produced_value = match (left, right) {
            (Value::Integer(left), Value::Integer(right)) => {
                left.checked_sub(*right).map_or(Value::Void, Value::Integer)
            }
            (Value::Real(left), Value::Real(right)) => Value::Real(left - *right),
            (Value::Real(_), other) => {
                return Err(Fault::type_mismatch(
                    "can't subtract @expected and `@received-value` (@received-type)",
                    ValueKind::Real,
                    other.clone(),
                ))
            }
            (Value::Integer(_), other) => {
                return Err(Fault::type_mismatch(
                    "can't subtract @expected and `@received-value` (@received-type)",
                    ValueKind::Integer,
                    other.clone(),
                ))
            }
            (other, _) => {
                return Err(Fault::invalid_type(
                    "`@received-value` (@received-type) is not able to be subtracted",
                    other.clone(),
                ))
            }
        };
        *self.resolve_value_source_mut(result)? = produced_value;
        Ok(None)
    }

    // floating point casts are intentional in this code.
    fn multiply(
        &mut self,
        left: ValueSource,
        right: &ValueOrSource,
        result: Destination,
    ) -> Result<Option<FlowControl>, Fault<'static, Env, Output>> {
        let left = self.resolve_value_source(left)?;
        let right = self.resolve_value_or_source(right)?;

        let produced_value = match (left, right) {
            (Value::Integer(left), Value::Integer(right)) => {
                left.checked_mul(*right).map_or(Value::Void, Value::Integer)
            }
            (Value::Real(left), Value::Real(right)) => Value::Real(left * *right),
            (Value::Real(_), other) => {
                return Err(Fault::type_mismatch(
                    "can't multiply @expected and `@received-value` (@received-type)",
                    ValueKind::Real,
                    other.clone(),
                ))
            }
            (Value::Integer(_), other) => {
                return Err(Fault::type_mismatch(
                    "can't multiply @expected and `@received-value` (@received-type)",
                    ValueKind::Integer,
                    other.clone(),
                ))
            }
            (other, _) => {
                return Err(Fault::invalid_type(
                    "`@received-value` (@received-type) is not able to be multiplied",
                    other.clone(),
                ))
            }
        };
        *self.resolve_value_source_mut(result)? = produced_value;
        Ok(None)
    }

    // floating point casts are intentional in this code.
    fn divide(
        &mut self,
        left: ValueSource,
        right: &ValueOrSource,
        result: Destination,
    ) -> Result<Option<FlowControl>, Fault<'static, Env, Output>> {
        let left = self.resolve_value_source(left)?;
        let right = self.resolve_value_or_source(right)?;

        let produced_value = match (left, right) {
            (Value::Integer(left), Value::Integer(right)) => {
                left.checked_div(*right).map_or(Value::Void, Value::Integer)
            }
            (Value::Real(left), Value::Real(right)) => Value::Real(left / *right),
            (Value::Real(_), other) => {
                return Err(Fault::type_mismatch(
                    "can't divide @expected and `@received-value` (@received-type)",
                    ValueKind::Real,
                    other.clone(),
                ))
            }
            (Value::Integer(_), other) => {
                return Err(Fault::type_mismatch(
                    "can't divide @expected and `@received-value` (@received-type)",
                    ValueKind::Integer,
                    other.clone(),
                ))
            }
            (other, _) => {
                return Err(Fault::invalid_type(
                    "`@received-value` (@received-type) is not able to be divided",
                    other.clone(),
                ))
            }
        };
        *self.resolve_value_source_mut(result)? = produced_value;
        Ok(None)
    }

    fn r#if(
        &mut self,
        value: ValueSource,
        false_jump_to: usize,
    ) -> Result<Option<FlowControl>, Fault<'static, Env, Output>> {
        if self.resolve_value_source(value)?.is_truthy() {
            Ok(None)
        } else {
            Ok(Some(FlowControl::JumpTo(false_jump_to)))
        }
    }

    #[allow(clippy::unnecessary_wraps)] // makes caller more clean
    fn equality<const INVERSE: bool>(left: &Value, right: &Value) -> bool {
        let mut result = left.eq(right);
        if INVERSE {
            result = !result;
        }
        result
    }

    fn compare_values(
        left: &Value,
        right: &Value,
        matcher: impl FnOnce(Ordering) -> bool,
    ) -> Result<bool, Fault<'static, Env, Output>> {
        if let Some(ordering) = left.partial_cmp(right) {
            Ok(matcher(ordering))
        } else {
            Err(Fault::type_mismatch(
                "invalid comparison between @expected and `@received-value` (@received-type)",
                left.kind(),
                right.clone(),
            ))
        }
    }

    fn compare(
        &mut self,
        comparison: Comparison,
        left: ValueSource,
        right: &ValueOrSource,
        result: CompareAction,
    ) -> Result<Option<FlowControl>, Fault<'static, Env, Output>> {
        let left = self.resolve_value_source(left)?;
        let right = self.resolve_value_or_source(right)?;

        let comparison_result = match comparison {
            Comparison::Equal => Self::equality::<false>(left, right),
            Comparison::NotEqual => Self::equality::<true>(left, right),
            Comparison::LessThan => {
                Self::compare_values(left, right, |ordering| ordering == Ordering::Less)?
            }
            Comparison::LessThanOrEqual => Self::compare_values(left, right, |ordering| {
                matches!(ordering, Ordering::Less | Ordering::Equal)
            })?,
            Comparison::GreaterThan => {
                Self::compare_values(left, right, |ordering| ordering == Ordering::Greater)?
            }
            Comparison::GreaterThanOrEqual => Self::compare_values(left, right, |ordering| {
                matches!(ordering, Ordering::Greater | Ordering::Equal)
            })?,
        };

        match result {
            CompareAction::Store(dest) => {
                *self.resolve_value_source_mut(dest)? = Value::Boolean(comparison_result);

                Ok(None)
            }
            CompareAction::JumpIfFalse(target) => {
                if comparison_result {
                    Ok(None)
                } else {
                    Ok(Some(FlowControl::JumpTo(target)))
                }
            }
        }
    }

    fn push_var(
        &mut self,
        variable: usize,
    ) -> Result<Option<FlowControl>, Fault<'static, Env, Output>> {
        if let Some(stack_offset) = self.variables_offset.checked_add(variable) {
            if stack_offset < self.return_offset {
                let value = self.stack[stack_offset].clone();
                self.stack.push(value)?;
                return Ok(None);
            }
        }
        Err(Fault::from(FaultKind::InvalidVariableIndex))
    }

    fn push_arg(&mut self, arg: usize) -> Result<Option<FlowControl>, Fault<'static, Env, Output>> {
        if let Some(stack_offset) = self.arg_offset.checked_add(arg) {
            if stack_offset < self.variables_offset {
                let value = self.stack[stack_offset].clone();
                self.stack.push(value)?;
                return Ok(None);
            }
        }
        Err(Fault::from(FaultKind::InvalidArgumentIndex))
    }

    fn load(
        &mut self,
        variable: usize,
        value: &ValueOrSource,
    ) -> Result<Option<FlowControl>, Fault<'static, Env, Output>> {
        let value = self.resolve_value_or_source(value)?;
        *self.resolve_variable_mut(variable)? = value.clone();

        Ok(None)
    }

    fn call(
        &mut self,
        vtable_index: Option<usize>,
        arg_count: usize,
        destination: Destination,
    ) -> Result<Option<FlowControl>, Fault<'static, Env, Output>> {
        let vtable_index = vtable_index
            .or(self.vtable_index)
            .ok_or(FaultKind::InvalidVtableIndex)?;
        let function = &self
            .module
            .vtable
            .get(vtable_index)
            .ok_or(FaultKind::InvalidVtableIndex)?;

        assert_eq!(function.arg_count, arg_count);

        let variables_offset = self.stack.len();
        let return_offset = variables_offset + function.variable_count;
        let arg_offset = variables_offset - function.arg_count;
        if function.variable_count > 0 {
            self.stack.grow_to(return_offset)?;
        }

        let mut frame = StackFrame {
            module: self.module,
            stack: self.stack,
            environment: self.environment,
            return_offset,
            destination,
            variables_offset,
            arg_offset,
            return_value: None,
            vtable_index: Some(vtable_index),
            operation_index: 0,
            _output: PhantomData,
        };
        let returned_value = frame.execute_operations(&function.code)?;

        self.clean_stack_after_call(arg_offset, destination, returned_value)?;

        Ok(None)
    }

    fn call_instance(
        &mut self,
        target: Option<ValueSource>,
        name: &Symbol,
        arg_count: usize,
        destination: Destination,
    ) -> Result<Option<FlowControl>, Fault<'static, Env, Output>> {
        // To prevent overlapping a mutable borrow of the value plus immutable
        // borrows of the stack, we temporarily take the value from where it
        // lives.
        let stack_index = match target {
            Some(ValueSource::Argument(index)) => {
                if let Some(stack_index) = self.arg_offset.checked_add(index) {
                    if stack_index < self.variables_offset {
                        stack_index
                    } else {
                        return Err(Fault::from(FaultKind::InvalidArgumentIndex));
                    }
                } else {
                    return Err(Fault::from(FaultKind::InvalidArgumentIndex));
                }
            }
            Some(ValueSource::Variable(index)) => {
                if let Some(stack_index) = self.variables_offset.checked_add(index) {
                    if stack_index < self.return_offset {
                        stack_index
                    } else {
                        return Err(Fault::from(FaultKind::InvalidVariableIndex));
                    }
                } else {
                    return Err(Fault::from(FaultKind::InvalidVariableIndex));
                }
            }
            None => {
                // If None, the target is the value prior to the arguments.
                if let Some(stack_index) = self
                    .stack
                    .len()
                    .checked_sub(arg_count)
                    .and_then(|index| index.checked_sub(1))
                {
                    if stack_index >= self.return_offset {
                        stack_index
                    } else {
                        return Err(Fault::stack_underflow());
                    }
                } else {
                    return Err(Fault::stack_underflow());
                }
            }
        };

        // Verify the argument list is valid.
        let return_offset = self.stack.len();
        let arg_offset = return_offset.checked_sub(arg_count);
        match arg_offset {
            Some(arg_offset) if arg_offset >= self.return_offset => {}
            _ => return Err(Fault::stack_underflow()),
        };

        // Pull the target out of its current location.
        let mut target_value = Value::Void;
        std::mem::swap(&mut target_value, &mut self.stack[stack_index]);
        // Call without resolving any errors
        let result = match &mut target_value {
            Value::Dynamic(value) => value.call(name, self.stack.pop_n(arg_count)),

            _ => {
                return Err(Fault::from(FaultKind::invalid_type(
                    "@received-kind does not support function calls",
                    target_value,
                )))
            }
        };
        if target.is_some() {
            // Return the target to its proper location
            std::mem::swap(&mut target_value, &mut self.stack[stack_index]);
        } else {
            // Remove the target's stack space. We didn't do this earlier
            // because it would have caused a copy of all args. But at this
            // point, all the args have been drained during the call so the
            // target can simply be popped.
            self.stack.pop()?;
        }

        // If there was a fault, return.
        let produced_value = result?;
        match destination {
            Destination::Variable(variable) => {
                *self.resolve_variable_mut(variable)? = produced_value;
            }
            Destination::Stack => {
                self.stack.push(produced_value)?;
            }
            Destination::Return => {
                self.return_value = Some(produced_value);
            }
        }

        Ok(None)
    }
}

/// An unexpected event occurred while executing the virtual machine.
#[derive(Debug)]
pub struct Fault<'a, Env, ReturnType> {
    /// The kind of fault this is.
    pub kind: FaultOrPause<'a, Env, ReturnType>,
    /// The stack trace of the virtual machine when the fault was raised.
    pub stack: Vec<FaultStackFrame>,
}

impl<'a, Env, ReturnType> Fault<'a, Env, ReturnType> {
    fn stack_underflow() -> Self {
        Self::from(FaultKind::StackUnderflow)
    }

    fn invalid_type(message: impl Into<String>, received: Value) -> Self {
        Self::from(FaultKind::invalid_type(message, received))
    }

    fn type_mismatch(message: impl Into<String>, expected: ValueKind, received: Value) -> Self {
        Self::from(FaultKind::type_mismatch(message, expected, received))
    }
}

impl<'a, Env, ReturnType> From<FaultKind> for Fault<'a, Env, ReturnType> {
    fn from(kind: FaultKind) -> Self {
        Self {
            kind: FaultOrPause::Fault(kind),
            stack: Vec::default(),
        }
    }
}

impl<'a, Env, ReturnType> std::error::Error for Fault<'a, Env, ReturnType>
where
    Env: std::fmt::Debug,
    ReturnType: std::fmt::Debug,
{
}

impl<'a, Env, ReturnType> Display for Fault<'a, Env, ReturnType> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.kind {
            FaultOrPause::Fault(fault) => Display::fmt(fault, f),
            FaultOrPause::Pause(_) => f.write_str("paused execution"),
        }
    }
}

/// A reason for a virtual machine [`Fault`].
#[derive(Debug)]
pub enum FaultOrPause<'a, Env, ReturnType> {
    /// A fault occurred while processing instructions.
    Fault(FaultKind),
    /// Execution was paused by the [`Environment`] as a result of returning
    /// [`ExecutionBehavior::Pause`] from [`Environment::step`].
    ///
    /// The contained [`PausedExecution`] can be used to resume executing the
    /// code.
    Pause(PausedExecution<'a, Env, ReturnType>),
}

/// An unexpected event within the virtual machine.
#[derive(Debug)]
pub enum FaultKind {
    /// An attempt to push a value to the stack was made after the stack has
    /// reached its capacity.
    StackOverflow,
    /// An attempt to pop a value off of the stack was made when no values were
    /// present in the current code's stack frame.
    StackUnderflow,
    /// An invalid variable index was used.
    InvalidVariableIndex,
    /// An invalid argument index was used.
    InvalidArgumentIndex,
    /// An invalid vtable index was used.
    InvalidVtableIndex,
    /// A call was made to a function that does not exist.
    UnknownFunction {
        /// The kind of the value the function was called on.
        kind: ValueKind,
        /// The name of the function being called.
        name: Symbol,
    },
    /// A value type was unexpected in the given context.
    TypeMismatch {
        /// The error message explaining the type mismatch.
        ///
        /// These patterns will be replaced in `message` before being Displayed:
        ///
        /// - @expected
        /// - @received-value
        /// - @received-kind
        message: String,
        /// The kind expected in this context.
        expected: ValueKind,
        /// The value that caused an error.
        received: Value,
    },
    /// An invalid value type was encountered.
    InvalidType {
        /// The error message explaining the type mismatch.
        ///
        /// These patterns will be replaced in `message` before being Displayed:
        ///
        /// - @received-value
        /// - @received-kind
        message: String,
        /// The value that caused an error.
        received: Value,
    },
    /// An error arose from dynamic types.
    Dynamic(DynamicFault),
}

impl FaultKind {
    fn invalid_type(message: impl Into<String>, received: Value) -> Self {
        FaultKind::InvalidType {
            message: message.into(),
            received,
        }
    }

    fn type_mismatch(message: impl Into<String>, expected: ValueKind, received: Value) -> Self {
        FaultKind::TypeMismatch {
            message: message.into(),
            expected,
            received,
        }
    }
}

impl std::error::Error for FaultKind {}

impl Display for FaultKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FaultKind::StackOverflow => f.write_str("stack pushed to while at maximum capacity"),
            FaultKind::StackUnderflow => f.write_str("stack popped but no values present"),
            FaultKind::InvalidVariableIndex => {
                f.write_str("a variable index was outside of the range allocated for the function")
            }
            FaultKind::InvalidArgumentIndex => f.write_str(
                "an argument index was beyond the number of arguments passed to the function",
            ),
            FaultKind::InvalidVtableIndex => f.write_str(
                "a vtable index was beyond the number functions registerd in the current module",
            ),
            FaultKind::UnknownFunction { kind, name } => {
                write!(f, "unknown function {name} on {}", kind.as_str())
            }
            FaultKind::TypeMismatch {
                message,
                expected,
                received,
            } => {
                let message = message.replace("@expected", expected.as_str());
                let message = message.replace("@received-type", received.kind().as_str());
                let message = message.replace("@received-value", &received.to_string());
                f.write_str(&message)
            }
            FaultKind::InvalidType { message, received } => {
                let message = message.replace("@received-type", received.kind().as_str());
                let message = message.replace("@received-value", &received.to_string());
                f.write_str(&message)
            }
            FaultKind::Dynamic(dynamic) => dynamic.fmt(f),
        }
    }
}

/// A stack frame entry of a [`Fault`].
#[derive(Debug)]
pub struct FaultStackFrame {
    /// The vtable index of the function being executed. If None, the
    /// instructions being executed were passed directly into [`Bud::run()`].
    pub vtable_index: Option<usize>,
    /// The index of the instruction that was executing when this fault was
    /// raised.
    pub instruction_index: usize,
}

/// A paused code execution.
#[derive(Debug)]
pub struct PausedExecution<'a, Env, ReturnType> {
    context: Option<&'a mut Bud<Env>>,
    operations: Option<Cow<'a, [Instruction]>>,
    stack: VecDeque<PausedFrame>,
    _return: PhantomData<ReturnType>,
}

impl<'a, Env, ReturnType> PausedExecution<'a, Env, ReturnType>
where
    ReturnType: FromStack,
{
    /// Returns a reference to the [`Environment`] from the virtual machine that
    /// is paused.
    #[must_use]
    pub fn environment(&self) -> &Env {
        &self.context.as_ref().expect("context missing").environment
    }

    /// Returns a mutable reference to the [`Environment`] from the virtual
    /// machine that is paused.
    #[must_use]
    pub fn environment_mut(&mut self) -> &mut Env {
        &mut self.context.as_mut().expect("context missing").environment
    }

    /// Resumes executing the virtual machine.
    pub fn resume(self) -> Result<ReturnType, Fault<'a, Env, ReturnType>>
    where
        Env: Environment,
    {
        let context = self
            .context
            .expect("context should be present before returning to the user");
        let operations = self
            .operations
            .expect("operations should be present before returning to the user");
        context.resume(operations, self.stack)
    }
}

#[derive(Debug)]
struct PausedFrame {
    return_offset: usize,
    arg_offset: usize,
    variables_offset: usize,

    vtable_index: Option<usize>,
    operation_index: usize,
    destination: Destination,
}

/// A type that can be constructed from popping from the virtual machine stack.
pub trait FromStack: Sized {
    /// Returns an instance constructing from the stack.
    fn from_value(value: Value) -> Result<Self, FaultKind>;
}

impl FromStack for Value {
    fn from_value(value: Value) -> Result<Self, FaultKind> {
        Ok(value)
    }
}

impl FromStack for i64 {
    fn from_value(value: Value) -> Result<Self, FaultKind> {
        match value {
            Value::Integer(integer) => Ok(integer),
            other => Err(FaultKind::type_mismatch(
                "@expected expected but received `@received-value` (@received-type)",
                ValueKind::Integer,
                other,
            )),
        }
    }
}

impl FromStack for f64 {
    fn from_value(value: Value) -> Result<Self, FaultKind> {
        match value {
            Value::Real(number) => Ok(number),
            other => Err(FaultKind::type_mismatch(
                "@expected expected but received `@received-value` (@received-type)",
                ValueKind::Real,
                other,
            )),
        }
    }
}

impl FromStack for bool {
    fn from_value(value: Value) -> Result<Self, FaultKind> {
        match value {
            Value::Boolean(value) => Ok(value),
            other => Err(FaultKind::type_mismatch(
                "@expected expected but received `@received-value` (@received-type)",
                ValueKind::Boolean,
                other,
            )),
        }
    }
}

impl FromStack for () {
    fn from_value(_value: Value) -> Result<Self, FaultKind> {
        Ok(())
    }
}

impl<T> FromStack for T
where
    T: DynamicValue,
{
    fn from_value(value: Value) -> Result<Self, FaultKind> {
        value.into_dynamic().map_err(|value| {
            FaultKind::type_mismatch("invalid type", ValueKind::Dynamic(type_name::<T>()), value)
        })
    }
}

/// A Rust value that has been wrapped for use in the virtual machine.
#[derive(Clone, Debug)]
pub struct Dynamic(
    // The reason for `Arc<Box<dyn UnboxableDynamicValue>>` instead of `Arc<dyn
    // UnboxableDynamicValue>` is the size. `Arc<dyn>` uses a fat pointer which
    // results in 16-bytes being used. By boxing the `dyn` value first, the Arc
    // pointer becomes a normal pointer resulting in this type being only 8
    // bytes.
    Arc<Box<dyn UnboxableDynamicValue>>,
);

#[test]
fn sizes() {
    assert_eq!(
        std::mem::size_of::<Dynamic>(),
        std::mem::size_of::<*const u8>()
    );
    assert_eq!(
        std::mem::size_of::<Symbol>(),
        std::mem::size_of::<*const u8>()
    );
    assert_eq!(
        std::mem::size_of::<Value>(),
        std::mem::size_of::<(*const u8, usize)>()
    );
}

impl Dynamic {
    fn new(value: impl DynamicValue + 'static) -> Self {
        Self(Arc::new(Box::new(DynamicValueData(Some(value)))))
    }

    fn as_mut(&mut self) -> &mut Box<dyn UnboxableDynamicValue> {
        if Arc::strong_count(&self.0) > 1 {
            // More than one reference to this Arc, we have to create a
            // clone instead. We do this before due to overlapping lifetime
            // issues using get_mut twice. We can't use make_mut due to
            // Box<dyn> not being cloneable.
            let new_value = self.0.cloned();
            self.0 = Arc::new(new_value);
        }

        Arc::get_mut(&mut self.0).expect("checked strong count") // This will need to change if we ever allow weak references.
    }

    fn call(&mut self, name: &Symbol, arguments: PoppedValues<'_>) -> Result<Value, FaultKind> {
        self.as_mut().call(name, arguments)
    }
}

impl Display for Dynamic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

trait UnboxableDynamicValue: Debug + Display {
    fn cloned(&self) -> Box<dyn UnboxableDynamicValue>;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn as_opt_any_mut(&mut self) -> &mut dyn Any;

    fn is_truthy(&self) -> bool;
    fn kind(&self) -> &'static str;
    fn partial_eq(&self, other: &Value) -> Option<bool>;
    fn partial_cmp(&self, other: &Value) -> Option<Ordering>;
    fn call(&mut self, name: &Symbol, arguments: PoppedValues<'_>) -> Result<Value, FaultKind>;
}

#[derive(Clone)]
struct DynamicValueData<T>(Option<T>);

impl<T> DynamicValueData<T> {
    #[inline]
    fn value(&self) -> &T {
        self.0.as_ref().expect("value taken")
    }
    #[inline]
    fn value_mut(&mut self) -> &mut T {
        self.0.as_mut().expect("value taken")
    }
}

impl<T> UnboxableDynamicValue for DynamicValueData<T>
where
    T: DynamicValue + Any + Debug,
{
    fn cloned(&self) -> Box<dyn UnboxableDynamicValue> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self.value()
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self.value_mut()
    }

    fn as_opt_any_mut(&mut self) -> &mut dyn Any {
        &mut self.0
    }

    fn is_truthy(&self) -> bool {
        self.value().is_truthy()
    }

    fn kind(&self) -> &'static str {
        self.value().kind()
    }

    fn partial_eq(&self, other: &Value) -> Option<bool> {
        self.value().partial_eq(other)
    }

    fn partial_cmp(&self, other: &Value) -> Option<Ordering> {
        self.value().partial_cmp(other)
    }

    fn call(&mut self, name: &Symbol, arguments: PoppedValues<'_>) -> Result<Value, FaultKind> {
        self.value_mut().call(name, arguments)
    }
}

impl<T> DynamicValue for DynamicValueData<T>
where
    T: DynamicValue,
{
    fn is_truthy(&self) -> bool {
        self.value().is_truthy()
    }

    fn kind(&self) -> &'static str {
        self.value().kind()
    }

    fn partial_eq(&self, other: &Value) -> Option<bool> {
        self.value().partial_eq(other)
    }

    fn partial_cmp(&self, other: &Value) -> Option<Ordering> {
        self.value().partial_cmp(other)
    }
}

impl<T> Debug for DynamicValueData<T>
where
    T: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(value) = self.0.as_ref() {
            Debug::fmt(value, f)
        } else {
            f.debug_struct("DynamicValueData").finish_non_exhaustive()
        }
    }
}

impl<T> Display for DynamicValueData<T>
where
    T: Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(value) = self.0.as_ref() {
            Display::fmt(value, f)
        } else {
            Ok(())
        }
    }
}

/// Customizes the behavior of a virtual machine instance.
pub trait Environment: 'static {
    /// Called once before each instruction is executed.
    ///
    /// If [`ExecutionBehavior::Continue`] is returned, the next instruction
    /// will be exected.
    ///
    /// If [`ExecutionBehavior::Pause`] is returned, the virtual machine is
    /// paused and a [`FaultOrPause::Pause`] is raised. If the execution is
    /// resumed, the first function call will be before executing the same
    /// instruction as the one when [`ExecutionBehavior::Pause`] was called.
    fn step(&mut self) -> ExecutionBehavior;
}

impl Environment for () {
    #[inline]
    fn step(&mut self) -> ExecutionBehavior {
        ExecutionBehavior::Continue
    }
}

/// An [`Environment`] that allows executing an amount of instructions before
/// pausing the virtual machine.
#[derive(Debug, Default)]
#[must_use]
pub struct Budgeted(usize);

impl Budgeted {
    /// Returns a new instance with the provided initial budget.
    pub const fn new(initial_budget: usize) -> Self {
        Self(initial_budget)
    }

    /// Returns the current balance of the budget.
    #[must_use]
    pub const fn balance(&self) -> usize {
        self.0
    }

    /// Adds an additional budget. This value will saturate `usize` instead of
    /// panicking or overflowing.
    pub fn add_budget(&mut self, additional_budget: usize) {
        self.0 = self.0.saturating_add(additional_budget);
    }
}

impl Environment for Budgeted {
    #[inline]
    fn step(&mut self) -> ExecutionBehavior {
        if self.0 > 0 {
            self.0 -= 1;
            ExecutionBehavior::Continue
        } else {
            ExecutionBehavior::Pause
        }
    }
}

/// The virtual machine behavior returned from [`Environment::step()`].
pub enum ExecutionBehavior {
    /// The virtual machine should continue executing.
    Continue,
    /// The virtual machine should pause before the next instruction is
    /// executed.
    Pause,
}

// #[test]
// fn budget() {
//     let mut context = Bud::default_for(Budgeted::new(0));
//     let mut fault = context
//         .run::<i64>(Cow::Borrowed(&[
//             Instruction::Push(Value::Integer(1)),
//             Instruction::Push(Value::Integer(2)),
//             Instruction::Add,
//         ]))
//         .unwrap_err();
//     let output = loop {
//         println!("Paused");
//         let mut pending = match fault.kind {
//             FaultOrPause::Pause(pending) => pending,
//             FaultOrPause::Fault(error) => unreachable!("unexpected error: {error}"),
//         };
//         pending.environment_mut().add_budget(1);

//         fault = match pending.resume() {
//             Ok(result) => break result,
//             Err(err) => err,
//         };
//     };

//     assert_eq!(output, 3);
// }

#[test]
fn budget_with_frames() {
    let test = Function {
        arg_count: 1,
        variable_count: 2,
        code: vec![
            Instruction::If {
                condition: ValueSource::Argument(0),
                false_jump_to: 12,
            },
            Instruction::Load {
                variable_index: 0,
                value: ValueOrSource::Value(Value::Integer(1)),
            },
            Instruction::Push(Value::Integer(1)),
            Instruction::Push(Value::Integer(2)),
            Instruction::Add {
                left: ValueSource::Variable(0),
                right: ValueOrSource::Value(Value::Integer(2)),
                destination: Destination::Variable(0),
            },
            Instruction::Push(Value::Integer(3)),
            Instruction::Add {
                left: ValueSource::Variable(0),
                right: ValueOrSource::Value(Value::Integer(3)),
                destination: Destination::Variable(0),
            },
            Instruction::Push(Value::Integer(4)),
            Instruction::Add {
                left: ValueSource::Variable(0),
                right: ValueOrSource::Value(Value::Integer(4)),
                destination: Destination::Variable(0),
            },
            Instruction::Push(Value::Integer(5)),
            Instruction::Add {
                left: ValueSource::Variable(0),
                right: ValueOrSource::Value(Value::Integer(5)),
                destination: Destination::Variable(0),
            },
            Instruction::Return(Some(ValueOrSource::Variable(0))),
            // If we were passed false, call ourself twice.
            Instruction::Push(Value::Boolean(true)),
            Instruction::Call {
                vtable_index: None,
                arg_count: 1,
                destination: Destination::Variable(0),
            },
            Instruction::Push(Value::Boolean(true)),
            Instruction::Call {
                vtable_index: None,
                arg_count: 1,
                destination: Destination::Variable(1),
            },
            Instruction::Add {
                left: ValueSource::Variable(0),
                right: ValueOrSource::Variable(1),
                destination: Destination::Variable(0),
            }, // should produce 30
            Instruction::PushCopy(ValueSource::Variable(0)),
        ],
    };
    let mut context = Bud::default_for(Budgeted::new(0)).with_function("test", test);
    let mut fault = context
        .run::<i64>(
            Cow::Borrowed(&[
                Instruction::Push(Value::Boolean(false)),
                Instruction::Call {
                    vtable_index: Some(0),
                    arg_count: 1,
                    destination: Destination::Stack,
                },
            ]),
            0,
        )
        .unwrap_err();
    let output = loop {
        println!("Paused");
        let mut pending = match fault.kind {
            FaultOrPause::Pause(pending) => pending,
            FaultOrPause::Fault(error) => unreachable!("unexpected error: {error}"),
        };
        pending.environment_mut().add_budget(1);

        fault = match pending.resume() {
            Ok(result) => break result,
            Err(err) => err,
        };
    };

    assert_eq!(output, 30);
}

/// A stack of [`Value`]s.
#[derive(Debug)]
pub struct Stack {
    values: Vec<Value>,
    length: usize,
    remaining_capacity: usize,
}

impl Default for Stack {
    fn default() -> Self {
        Self {
            values: Vec::default(),
            length: 0,
            remaining_capacity: usize::MAX,
        }
    }
}

impl Stack {
    /// Returns a new stack with enough reserved space to store
    /// `initial_capacity` values without reallocating and will not allow
    /// pushing more than `maximum_capacity` values.
    #[must_use]
    pub fn new(initial_capacity: usize, maximum_capacity: usize) -> Self {
        Self {
            values: Vec::with_capacity(initial_capacity),
            length: 0,
            remaining_capacity: maximum_capacity,
        }
    }

    /// Pushes `value` to the stack.
    ///
    /// # Errors
    ///
    /// Returns [`FaultKind::StackOverflow`] if the stack's maximum capacity has
    /// been reached.
    #[inline]
    pub fn push(&mut self, value: Value) -> Result<(), FaultKind> {
        if self.remaining_capacity > 0 {
            self.remaining_capacity -= 1;
            if self.length < self.values.len() {
                self.values[self.length] = value;
            } else {
                self.values.push(value);
            }
            self.length += 1;
            Ok(())
        } else {
            Err(FaultKind::StackOverflow)
        }
    }

    /// Pushes multiple arguments to the stack.
    pub fn extend<Args, ArgsIter>(&mut self, args: Args) -> Result<usize, FaultKind>
    where
        Args: IntoIterator<Item = Value, IntoIter = ArgsIter>,
        ArgsIter: Iterator<Item = Value> + ExactSizeIterator + DoubleEndedIterator,
    {
        let mut args = args.into_iter().rev();
        let arg_count = args.len();
        if self.remaining_capacity >= arg_count {
            self.remaining_capacity -= arg_count;
            let new_length = self.length + arg_count;
            let current_vec_length = self.values.len();
            if new_length < current_vec_length {
                // We can replace the existing values in this range
                self.values.splice(self.length..new_length, args);
            } else {
                while self.length < current_vec_length {
                    self.values[self.length] = args.next().expect("length checked");
                    self.length += 1;
                }
                // The remaining can be added to the end of the vec
                self.values.extend(args);
            }

            self.length = new_length;

            Ok(arg_count)
        } else {
            Err(FaultKind::StackOverflow)
        }
    }

    /// Pops `count` elements from the top of the stack.
    ///
    /// This iterator returns the values in the sequence that they are ordered
    /// on the stack, which is different than calling pop() `count` times
    /// sequentially. For example, if the stack contains `[0, 1, 2, 3]`, calling
    /// pop() twice will result in `3, 2`. Calling `pop_n(2)` will result in `2,
    /// 3`.
    pub fn pop_n(&mut self, count: usize) -> PoppedValues<'_> {
        // Make sure we aren't trying to pop too many
        let end = self.length;
        let count = count.min(end);
        let start = end - count;
        self.remaining_capacity += count;
        self.length -= count;
        PoppedValues {
            stack: self,
            current: start,
            end,
        }
    }

    /// Returns a reference to the top [`Value`] on the stack, or returns a
    /// [`FaultKind::StackUnderflow`] if no values are present.
    #[inline]
    pub fn top(&self) -> Result<&Value, FaultKind> {
        if self.length > 0 {
            Ok(&self.values[self.length])
        } else {
            Err(FaultKind::StackUnderflow)
        }
    }

    /// Returns a reference to the top [`Value`] on the stack, or returns a
    /// [`FaultKind::StackUnderflow`] if no values are present.
    #[inline]
    pub fn top_mut(&mut self) -> Result<&mut Value, FaultKind> {
        if self.length > 0 {
            Ok(&mut self.values[self.length - 1])
        } else {
            Err(FaultKind::StackUnderflow)
        }
    }

    /// Pops a [`Value`] from the stack.
    ///
    /// # Errors
    ///
    /// Returns [`FaultKind::StackUnderflow`] if the stack is empty.
    #[inline]
    pub fn pop(&mut self) -> Result<Value, FaultKind> {
        if let Some(new_length) = self.length.checked_sub(1) {
            let value = std::mem::take(&mut self.values[new_length]);
            self.remaining_capacity += 1;
            self.length = new_length;
            Ok(value)
        } else {
            Err(FaultKind::StackUnderflow)
        }
    }

    // /// Pops a [`Value`] from the stack and returns a mutable reference to the
    // /// next value.
    // ///
    // /// # Errors
    // ///
    // /// Returns [`FaultKind::StackUnderflow`] if the stack does not contain at
    // /// least two values.
    // #[inline]
    // pub fn pop_and_modify(&mut self) -> Result<(Value, &mut Value), FaultKind> {
    //     if self.values.len() >= 2 {
    //         let first = self.values.pop().expect("bounds already checked");
    //         self.remaining_capacity += 1;

    //         Ok((
    //             first,
    //             self.values.last_mut().expect("bounds already checked"),
    //         ))
    //     } else {
    //         Err(FaultKind::StackUnderflow)
    //     }
    // }

    /// Returns the number of [`Value`]s contained in this stack.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.length
    }

    /// Returns true if this stack has no values.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the number of [`Value`]s that can be pushed to this stack before
    /// a [`FaultKind::StackOverflow`] is raised.
    #[must_use]
    pub const fn remaining_capacity(&self) -> usize {
        self.remaining_capacity
    }

    #[inline]
    fn remove_range<R>(&mut self, range: R)
    where
        R: RangeBounds<usize>,
    {
        let mut start = match range.start_bound() {
            Bound::Included(start) => *start,
            Bound::Excluded(start) => start.saturating_sub(1),
            Bound::Unbounded => 0,
        };
        let end = match range.end_bound() {
            Bound::Included(end) => end.saturating_add(1),
            Bound::Excluded(end) => *end,
            Bound::Unbounded => self.length,
        };
        let removed = end - start;
        if removed > 0 {
            if let Some(values_to_copy) = self.length.checked_sub(end) {
                // We have values at the end we should copy first
                for index in 0..values_to_copy {
                    let value = std::mem::take(&mut self.values[end + index]);
                    self.values[start + index] = value;
                }
                start += values_to_copy;
            }
            // Replace the values with Void to free any ref-counted values.
            if end > start {
                // For some odd reason, fill_with is faster than fill here.
                self.values[start..end].fill_with(|| Value::Void);
            }
        }
        self.remaining_capacity += removed;
        self.length -= removed;
    }

    fn clear(&mut self) {
        if self.length > 0 {
            self.values[0..self.length].fill_with(|| Value::Void);
        }
        self.remaining_capacity += self.length;
        self.length = 0;
    }

    #[inline]
    fn grow_to(&mut self, new_size: usize) -> Result<(), FaultKind> {
        let extra_capacity = new_size.saturating_sub(self.length);
        if let Some(remaining_capacity) = self.remaining_capacity.checked_sub(extra_capacity) {
            self.remaining_capacity = remaining_capacity;
            if new_size >= self.values.len() {
                self.values.resize_with(new_size, || Value::Void);
            }
            self.length = new_size;
            Ok(())
        } else {
            Err(FaultKind::StackOverflow)
        }
    }

    #[inline]
    fn grow_by(&mut self, additional_voids: usize) -> Result<(), FaultKind> {
        if let Some(remaining_capacity) = self.remaining_capacity.checked_sub(additional_voids) {
            self.remaining_capacity = remaining_capacity;
            let new_size = self.length + additional_voids;
            if new_size > self.values.len() {
                self.values.resize_with(new_size, || Value::Void);
            }
            self.length = new_size;
            Ok(())
        } else {
            Err(FaultKind::StackOverflow)
        }
    }
}

impl Index<usize> for Stack {
    type Output = Value;

    #[inline]
    fn index(&self, index: usize) -> &Self::Output {
        &self.values[index]
    }
}

impl IndexMut<usize> for Stack {
    #[inline]
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.values[index]
    }
}

/// An iterator over a sequence of values being removed from the top of a
/// [`Stack`].
///
/// This iterator returns the values in the sequence that they are ordered on
/// the stack, which is different than calling pop() `count` times sequentially.
/// For example, if the stack contains `[0, 1, 2, 3]`, calling pop() twice will
/// result in `3, 2`. Calling `pop_n(2)` will result in `2, 3`.
pub struct PoppedValues<'a> {
    stack: &'a mut Stack,
    end: usize,
    current: usize,
}

impl<'a> Drop for PoppedValues<'a> {
    fn drop(&mut self) {
        self.stack.values[self.current..self.end].fill_with(|| Value::Void);
    }
}

impl<'a> Iterator for PoppedValues<'a> {
    type Item = Value;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current < self.end {
            let result = Some(std::mem::take(&mut self.stack.values[self.current]));
            self.current += 1;
            result
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.end - self.current, None)
    }
}

impl<'a> ExactSizeIterator for PoppedValues<'a> {}

/// A [`Fault`] that arose from a [`Dynamic`] value.
#[derive(Debug)]
pub struct DynamicFault(Box<dyn AnyDynamicError>);

impl DynamicFault {
    /// Returns a new instance containing the provided error.
    pub fn new<T: Debug + Display + 'static>(error: T) -> Self {
        Self(Box::new(DynamicErrorContents(Some(error))))
    }

    /// Returns a reference to the original error, if `T` is the same type that
    /// was provided during construction.
    #[must_use]
    pub fn downcast_ref<T: Debug + Display + 'static>(&self) -> Option<&T> {
        self.0.as_any().downcast_ref()
    }

    /// Returns the original error if `T` is the same type that was provided
    /// during construction. If not, `Err(self)` will be returned.
    pub fn try_unwrap<T: Debug + Display + 'static>(mut self) -> Result<T, Self> {
        if let Some(opt_any) = self.0.as_opt_any_mut().downcast_mut::<Option<T>>() {
            Ok(std::mem::take(opt_any).expect("value already taken"))
        } else {
            Err(self)
        }
    }
}

#[test]
fn dynamic_error_conversions() {
    let error = DynamicFault::new(true);
    assert!(*error.downcast_ref::<bool>().unwrap());
    assert!(error.try_unwrap::<bool>().unwrap());
}

#[derive(Debug)]
struct DynamicErrorContents<T>(Option<T>);

impl<T> Display for DynamicErrorContents<T>
where
    T: Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(value) = self.0.as_ref() {
            Display::fmt(value, f)
        } else {
            Ok(())
        }
    }
}

trait AnyDynamicError: Debug + Display {
    fn as_any(&self) -> &dyn Any;
    fn as_opt_any_mut(&mut self) -> &mut dyn Any;
}

impl<T> AnyDynamicError for DynamicErrorContents<T>
where
    T: Debug + Display + 'static,
{
    fn as_any(&self) -> &dyn Any {
        self.0.as_ref().expect("value taken")
    }

    fn as_opt_any_mut(&mut self) -> &mut dyn Any {
        &mut self.0
    }
}

#[test]
fn invalid_variables() {
    let test = Function {
        arg_count: 0,
        variable_count: 0,
        code: vec![Instruction::PushCopy(ValueSource::Variable(0))],
    };
    let mut context = Bud::empty().with_function("test", test);
    assert!(matches!(
        context
            .run::<i64>(
                Cow::Borrowed(&[Instruction::Call {
                    vtable_index: Some(0),
                    arg_count: 0,
                    destination: Destination::Stack,
                }],),
                0
            )
            .unwrap_err()
            .kind,
        FaultOrPause::Fault(FaultKind::InvalidVariableIndex)
    ));
}

#[test]
fn invalid_argument() {
    let test = Function {
        arg_count: 0,
        variable_count: 0,
        code: vec![Instruction::PushCopy(ValueSource::Argument(0))],
    };
    let mut context = Bud::empty().with_function("test", test);
    assert!(matches!(
        context
            .run::<i64>(
                Cow::Borrowed(&[Instruction::Call {
                    vtable_index: Some(0),
                    arg_count: 0,
                    destination: Destination::Stack,
                }]),
                0
            )
            .unwrap_err()
            .kind,
        FaultOrPause::Fault(FaultKind::InvalidArgumentIndex)
    ));
}

#[test]
fn invalid_vtable_index() {
    let mut context = Bud::empty();
    assert!(matches!(
        context
            .run::<i64>(
                Cow::Borrowed(&[Instruction::Call {
                    vtable_index: Some(0),
                    arg_count: 0,
                    destination: Destination::Stack,
                }]),
                0
            )
            .unwrap_err()
            .kind,
        FaultOrPause::Fault(FaultKind::InvalidVtableIndex)
    ));
}

#[test]
fn function_without_return_value() {
    let test = Function {
        arg_count: 0,
        variable_count: 0,
        code: vec![],
    };
    let mut context = Bud::empty().with_function("test", test);
    assert_eq!(
        context
            .call::<Value, _, _>(&Symbol::from("test"), [])
            .unwrap(),
        Value::Void
    );
}

#[test]
fn function_needs_extra_cleanup() {
    let test = Function {
        arg_count: 0,
        variable_count: 0,
        code: vec![
            Instruction::Push(Value::Integer(1)),
            Instruction::Push(Value::Integer(2)),
        ],
    };
    let mut context = Bud::empty().with_function("test", test);
    assert_eq!(
        context
            .run::<Value>(
                Cow::Borrowed(&[Instruction::Call {
                    vtable_index: Some(0),
                    arg_count: 0,
                    destination: Destination::Stack,
                }]),
                0
            )
            .unwrap(),
        Value::Integer(1)
    );

    assert!(context.stack().is_empty());
}
