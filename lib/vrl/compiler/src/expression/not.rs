use std::{fmt, ptr::addr_of_mut};

use diagnostic::{DiagnosticMessage, Label, Note, Urls};

use crate::{
    expression::{Expr, Resolved},
    parser::Node,
    state::{ExternalEnv, LocalEnv},
    value::{Kind, VrlValueConvert},
    BatchContext, Context, Expression, Span, TypeDef,
};

pub(crate) type Result = std::result::Result<Not, Error>;

#[derive(Debug, Clone, PartialEq)]
pub struct Not {
    inner: Box<Expr>,
}

impl Not {
    pub fn new(node: Node<Expr>, not_span: Span, state: (&LocalEnv, &ExternalEnv)) -> Result {
        let (expr_span, expr) = node.take();
        let type_def = expr.type_def(state);

        if !type_def.is_boolean() {
            return Err(Error {
                variant: ErrorVariant::NonBoolean(type_def.into()),
                not_span,
                expr_span,
            });
        }

        Ok(Self {
            inner: Box::new(expr),
        })
    }
}

impl Expression for Not {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        Ok((!self.inner.resolve(ctx)?.try_boolean()?).into())
    }

    fn resolve_batch(&mut self, ctx: &mut BatchContext, selection_vector: &[usize]) {
        self.inner.resolve_batch(ctx, selection_vector);

        for index in selection_vector {
            let resolved = addr_of_mut!(ctx.resolved_values[*index]);
            let result = (|| Ok((!unsafe { resolved.read() }?.try_boolean()?).into()))();
            unsafe { resolved.write(result) };
        }
    }

    fn type_def(&self, state: (&LocalEnv, &ExternalEnv)) -> TypeDef {
        let fallible = self.inner.type_def(state).is_fallible();

        TypeDef::boolean().with_fallibility(fallible)
    }
}

impl fmt::Display for Not {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, r#"!{}"#, self.inner)
    }
}

// -----------------------------------------------------------------------------

#[derive(Debug)]
pub struct Error {
    pub(crate) variant: ErrorVariant,

    not_span: Span,
    expr_span: Span,
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum ErrorVariant {
    #[error("non-boolean negation")]
    NonBoolean(Kind),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#}", self.variant)
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.variant)
    }
}

impl DiagnosticMessage for Error {
    fn code(&self) -> usize {
        use ErrorVariant::NonBoolean;

        match &self.variant {
            NonBoolean(..) => 660,
        }
    }

    fn labels(&self) -> Vec<Label> {
        use ErrorVariant::NonBoolean;

        match &self.variant {
            NonBoolean(kind) => vec![
                Label::primary("negation only works on boolean values", self.not_span),
                Label::context(
                    format!("this expression resolves to {}", kind),
                    self.expr_span,
                ),
            ],
        }
    }

    fn notes(&self) -> Vec<Note> {
        use ErrorVariant::NonBoolean;

        match &self.variant {
            NonBoolean(..) => {
                vec![
                    Note::CoerceValue,
                    Note::SeeDocs(
                        "type coercion".to_owned(),
                        Urls::func_docs("#coerce-functions"),
                    ),
                ]
            }
        }
    }
}
