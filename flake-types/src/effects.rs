//! Effect set representation. Checking lands in Milestone 5.

use std::collections::BTreeSet;
use std::fmt;

/// A single effect kind.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Effect {
    Io,
    Alloc,
    Conc,
    Panic,
    Pure,
    Custom(String),
}

impl Effect {
    #[must_use]
    pub fn parse(name: &str) -> Self {
        match name {
            "io" => Self::Io,
            "alloc" => Self::Alloc,
            "conc" => Self::Conc,
            "panic" => Self::Panic,
            "pure" => Self::Pure,
            other => Self::Custom(other.to_string()),
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Io => "io",
            Self::Alloc => "alloc",
            Self::Conc => "conc",
            Self::Panic => "panic",
            Self::Pure => "pure",
            Self::Custom(s) => s,
        }
    }

    #[must_use]
    pub fn is_real(&self) -> bool {
        !matches!(self, Self::Pure)
    }
}

impl fmt::Display for Effect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Declared or inferred set of effects.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EffectSet {
    pub specified: bool,
    inner: BTreeSet<Effect>,
}

impl EffectSet {
    #[must_use]
    pub fn unspecified() -> Self {
        Self {
            specified: false,
            inner: BTreeSet::new(),
        }
    }

    #[must_use]
    pub fn pure() -> Self {
        Self {
            specified: true,
            inner: BTreeSet::new(),
        }
    }

    #[must_use]
    pub fn from_names<I, S>(names: I, specified: bool) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut inner = BTreeSet::new();
        for name in names {
            let effect = Effect::parse(name.as_ref());
            if effect.is_real() {
                inner.insert(effect);
            }
        }
        Self { specified, inner }
    }

    pub fn insert(&mut self, effect: Effect) {
        if effect.is_real() {
            self.inner.insert(effect);
        }
    }

    pub fn union_with(&mut self, other: &EffectSet) {
        self.inner.extend(other.inner.iter().cloned());
    }

    #[must_use]
    pub fn contains(&self, effect: &Effect) -> bool {
        self.inner.contains(effect)
    }

    #[must_use]
    pub fn is_subset(&self, other: &EffectSet) -> bool {
        self.inner.is_subset(&other.inner)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Effect> {
        self.inner.iter()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Effects a `main` function is allowed to perform.
    #[must_use]
    pub fn top_level() -> Self {
        let mut set = Self {
            specified: true,
            inner: BTreeSet::new(),
        };
        set.insert(Effect::Io);
        set.insert(Effect::Alloc);
        set.insert(Effect::Conc);
        set.insert(Effect::Panic);
        set
    }
}

impl fmt::Display for EffectSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.inner.is_empty() {
            return f.write_str("pure");
        }
        let mut first = true;
        for e in &self.inner {
            if !first {
                f.write_str(" + ")?;
            }
            first = false;
            write!(f, "{e}")?;
        }
        Ok(())
    }
}
