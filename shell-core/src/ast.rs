use crate::span::Span;

/// A complete shell program consisting of semicolon-separated statements.
#[derive(Debug, Clone)]
pub struct Program {
    /// The statements in the program.
    pub statements: Vec<Statement>,
    /// Source location of the entire program.
    pub span: Span,
}

/// A shell statement.
#[derive(Debug, Clone)]
pub enum Statement {
    /// Pipe expression: `left | right`.
    Pipe(PipeExpr),
    /// Short-circuit AND: `left && right`.
    And(Box<Self>, Box<Self>),
    /// Short-circuit OR: `left || right`.
    Or(Box<Self>, Box<Self>),
    /// If-then-elif-else conditional.
    If(IfStmt),
    /// While loop.
    While(WhileStmt),
    /// For loop over words.
    For(ForStmt),
    /// Case/switch statement.
    Case(CaseStmt),
    /// Function definition.
    FunctionDef(FunctionDef),
    /// Return from a function, optionally with a value.
    Return(Option<String>),
    /// Break out of a loop.
    Break(Span),
    /// Continue to the next loop iteration.
    Continue(Span),
    /// Braced compound list of statements.
    Compound(Vec<Self>, Span),
    /// Variable assignment.
    Assign(AssignStmt),
}

impl Statement {
    /// Returns the source span of this statement.
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Self::Pipe(p) => p.span,
            Self::And(l, r) | Self::Or(l, r) => Span::merge(l.span(), r.span()),
            Self::If(s) => s.span,
            Self::While(s) => s.span,
            Self::For(s) => s.span,
            Self::Case(s) => s.span,
            Self::FunctionDef(s) => s.span,
            Self::Return(_) => Span::dummy(),
            Self::Break(s) | Self::Continue(s) => *s,
            Self::Compound(_, s) => *s,
            Self::Assign(s) => s.span,
        }
    }
}

/// An `if` / `elif` / `else` conditional statement.
#[derive(Debug, Clone)]
pub struct IfStmt {
    /// The condition to evaluate.
    pub condition: Box<Statement>,
    /// Statements executed when the condition is true.
    pub body: Vec<Statement>,
    /// Additional `elif` branches, each with its own condition and body.
    pub elif_branches: Vec<(Box<Statement>, Vec<Statement>)>,
    /// Optional `else` branch body.
    pub else_body: Option<Vec<Statement>>,
    /// Source location.
    pub span: Span,
}

/// A `while` loop statement.
#[derive(Debug, Clone)]
pub struct WhileStmt {
    /// The loop condition.
    pub condition: Box<Statement>,
    /// The loop body.
    pub body: Vec<Statement>,
    /// Source location.
    pub span: Span,
}

/// A `for` loop that iterates over a list of words.
#[derive(Debug, Clone)]
pub struct ForStmt {
    /// The loop variable name.
    pub variable: String,
    /// The words to iterate over.
    pub words: Vec<String>,
    /// The loop body.
    pub body: Vec<Statement>,
    /// Source location.
    pub span: Span,
}

/// A `case` / `esac` switch statement.
#[derive(Debug, Clone)]
pub struct CaseStmt {
    /// The word to match against.
    pub word: String,
    /// The case arms.
    pub arms: Vec<CaseArm>,
    /// Source location.
    pub span: Span,
}

/// A single arm in a `case` statement.
#[derive(Debug, Clone)]
pub struct CaseArm {
    /// Patterns to match (each is a word, may contain wildcards).
    pub patterns: Vec<String>,
    /// Statements executed when a pattern matches.
    pub body: Vec<Statement>,
    /// Source location.
    pub span: Span,
}

/// A shell function definition.
#[derive(Debug, Clone)]
pub struct FunctionDef {
    /// The function name.
    pub name: String,
    /// The function body.
    pub body: Vec<Statement>,
    /// Source location.
    pub span: Span,
}

/// A variable assignment: `name=value`.
#[derive(Debug, Clone)]
pub struct AssignStmt {
    /// The variable name.
    pub name: String,
    /// The variable value.
    pub value: String,
    /// Source location.
    pub span: Span,
}

/// A pipeline of one or more atoms connected by `|`.
#[derive(Debug, Clone)]
pub struct PipeExpr {
    /// The pipeline atoms in order.
    pub atoms: Vec<Atom>,
    /// Source location.
    pub span: Span,
}

/// An element in a pipeline.
#[derive(Debug, Clone)]
pub enum Atom {
    /// A simple command.
    Command(SimpleCommand),
    /// A grouped (subshell or braced) compound.
    Group(Group),
}

impl Atom {
    /// Returns the source span of this atom.
    #[must_use]
    pub const fn span(&self) -> Span {
        match self {
            Self::Command(c) => c.span,
            Self::Group(g) => g.span,
        }
    }
}

/// A simple command with a name, arguments, and redirects.
#[derive(Debug, Clone)]
pub struct SimpleCommand {
    /// The command name or path.
    pub name: String,
    /// Arguments to the command.
    pub args: Vec<String>,
    /// Redirections attached to this command.
    pub redirects: Vec<Redirect>,
    /// Source location.
    pub span: Span,
}

/// A group of commands (subshell or braced group).
#[derive(Debug, Clone)]
pub struct Group {
    /// The parsed program inside the group.
    pub body: Program,
    /// Source location of the group.
    pub span: Span,
}

/// A shell redirection operator and its target.
#[derive(Debug, Clone)]
pub struct Redirect {
    /// Optional file descriptor number (e.g. `2` in `2>&1`).
    pub fd: Option<u32>,
    /// The kind of redirection.
    pub kind: RedirectKind,
    /// The target file or descriptor name.
    pub target: String,
    /// Source location.
    pub span: Span,
}

/// The type of a redirection operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedirectKind {
    /// Input redirection: `<file`.
    Input,
    /// Output redirection: `>file`.
    Output,
    /// Append output redirection: `>>file`.
    Append,
    /// Duplicate input fd: `<&fd`.
    FdInput,
    /// Duplicate output fd: `>&fd`.
    FdOutput,
    /// Append with fd duplication: `&>>file`.
    FdAppend,
    /// Duplicate an existing fd: `>&fd`.
    FdDup,
    /// Close an fd: `>&-`.
    FdClose,
    /// Here-document: `<<delim`.
    HereDoc,
    /// Here-string: `<<<word`.
    HereString,
}

impl RedirectKind {
    /// Returns the operator string for this redirection kind.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Input => "<",
            Self::Output => ">",
            Self::Append => ">>",
            Self::FdInput => "<&",
            Self::FdOutput => ">&",
            Self::FdAppend => "&>>",
            Self::FdDup => ">&",
            Self::FdClose => ">&-",
            Self::HereDoc => "<<",
            Self::HereString => "<<<",
        }
    }
}

impl std::fmt::Display for RedirectKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redirect_kind_display() {
        assert_eq!(format!("{}", RedirectKind::Input), "<");
        assert_eq!(format!("{}", RedirectKind::Output), ">");
        assert_eq!(format!("{}", RedirectKind::Append), ">>");
        assert_eq!(format!("{}", RedirectKind::HereDoc), "<<");
    }

    #[test]
    fn test_statement_span_pipe() {
        let span = Span::new(1, 1, 0, 5);
        let stmt = Statement::Pipe(PipeExpr {
            atoms: vec![],
            span,
        });
        assert_eq!(stmt.span(), span);
    }

    #[test]
    fn test_atom_span() {
        let span = Span::new(1, 1, 0, 3);
        let atom = Atom::Command(SimpleCommand {
            name: "ls".into(),
            args: vec![],
            redirects: vec![],
            span,
        });
        assert_eq!(atom.span(), span);
    }

    #[test]
    fn test_program_construction() {
        let prog = Program {
            statements: vec![Statement::Pipe(PipeExpr {
                atoms: vec![Atom::Command(SimpleCommand {
                    name: "echo".into(),
                    args: vec!["hello".into()],
                    redirects: vec![],
                    span: Span::new(1, 1, 0, 10),
                })],
                span: Span::new(1, 1, 0, 10),
            })],
            span: Span::new(1, 1, 0, 10),
        };
        assert_eq!(prog.statements.len(), 1);
    }
}
