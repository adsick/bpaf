use crate::Error;
use std::ffi::OsString;

/// Contains [`OsString`] with its [`String`] equivalent if encoding is utf8
#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct Word {
    pub utf8: Option<String>,
    pub os: OsString,
}

/// Hides [`Args`] internal implementation
mod inner {
    use std::{
        ffi::{OsStr, OsString},
        rc::Rc,
    };

    use super::{push_vec, Arg, Word};
    /// All currently present command line parameters
    #[derive(Clone, Debug)]
    pub struct Args {
        /// list of remaining arguments, for cheap cloning
        items: Rc<[Arg]>,
        /// removed items, false - present, true - removed
        removed: Vec<bool>,
        remaining: usize,

        /// Used to render an error message for [`parse`][crate::Parser::parse]
        pub(crate) current: Option<Word>,

        /// used to pick the parser that consumes the left most item
        pub(crate) head: usize,
    }

    impl<const N: usize> From<&[&str; N]> for Args {
        fn from(xs: &[&str; N]) -> Self {
            let vec = xs.iter().copied().collect::<Vec<_>>();
            Args::from(vec.as_slice())
        }
    }

    impl From<&[&str]> for Args {
        fn from(xs: &[&str]) -> Self {
            let mut pos_only = false;
            let mut vec = Vec::with_capacity(xs.len());
            for x in xs {
                push_vec(&mut vec, OsString::from(x), &mut pos_only);
            }
            Args::from(vec)
        }
    }

    impl From<&[&OsStr]> for Args {
        fn from(xs: &[&OsStr]) -> Self {
            let mut pos_only = false;
            let mut vec = Vec::with_capacity(xs.len());
            for x in xs {
                push_vec(&mut vec, OsString::from(x), &mut pos_only);
            }
            Args::from(vec)
        }
    }

    impl From<Vec<Arg>> for Args {
        fn from(vec: Vec<Arg>) -> Self {
            Args {
                removed: vec![false; vec.len()],
                remaining: vec.len(),
                items: Rc::from(vec),
                current: None,
                head: usize::MAX,
            }
        }
    }

    pub struct ArgsIter<'a> {
        args: &'a Args,
        cur: usize,
    }

    impl<'a> Args {
        /// creates iterator over remaining elements
        pub(crate) const fn items_iter(&'a self) -> ArgsIter<'a> {
            ArgsIter { args: self, cur: 0 }
        }

        pub(crate) fn remove(&mut self, index: usize) {
            if !self.removed[index] {
                self.remaining -= 1;
                self.head = self.head.min(index);
            }
            self.removed[index] = true;
        }

        pub(crate) const fn is_empty(&self) -> bool {
            self.remaining == 0
        }

        pub(crate) const fn len(&self) -> usize {
            self.remaining
        }
    }

    impl<'a> Iterator for ArgsIter<'a> {
        type Item = (usize, &'a Arg);

        fn next(&mut self) -> Option<Self::Item> {
            loop {
                let ix = self.cur;
                self.cur += 1;
                if !*self.args.removed.get(ix)? {
                    return Some((ix, &self.args.items[ix]));
                }
            }
        }
    }
}
pub use inner::*;

/// Preprocessed command line argument
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Arg {
    /// short flag
    Short(char),
    /// long flag
    Long(String),
    /// separate word that can be command, positional or an argument to a flag
    Word(Word),
}

impl std::fmt::Display for Arg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Arg::Short(s) => write!(f, "-{}", s),
            Arg::Long(l) => write!(f, "--{}", l),
            Arg::Word(w) => match &w.utf8 {
                Some(s) => write!(f, "{}", s),
                None => Err(std::fmt::Error),
            },
        }
    }
}

impl Arg {
    pub(crate) const fn is_short(&self, short: char) -> bool {
        match self {
            &Arg::Short(c) => c == short,
            Arg::Long(_) | Arg::Word(..) => false,
        }
    }

    pub(crate) fn is_long(&self, long: &str) -> bool {
        match self {
            Arg::Long(l) => long == *l,
            Arg::Short(_) | Arg::Word(..) => false,
        }
    }
}

pub(crate) fn push_vec(vec: &mut Vec<Arg>, os: OsString, pos_only: &mut bool) {
    // if we are after "--" sign or there's no utf8 representation for
    // an item - it can only be a positional argument
    let utf8 = match (*pos_only, os.to_str()) {
        (true, v) | (_, v @ None) => {
            return vec.push(Arg::Word(Word {
                utf8: v.map(String::from),
                os,
            }))
        }
        (false, Some(x)) => x,
    };

    if utf8 == "--" {
        *pos_only = true;
    } else if let Some(body) = utf8.strip_prefix("--") {
        if let Some((key, val)) = body.split_once('=') {
            vec.push(Arg::Long(key.to_owned()));
            vec.push(Arg::Word(Word {
                utf8: Some(val.to_owned()),
                os: OsString::from(val),
            }));
        } else {
            vec.push(Arg::Long(body.to_owned()));
        }
    } else if let Some(body) = utf8.strip_prefix('-') {
        if let Some((key, val)) = body.split_once('=') {
            assert_eq!(
                key.len(),
                1,
                "short flag with argument must have only one key"
            );
            let key = key.chars().next().expect("key should be one character");
            vec.push(Arg::Short(key));
            vec.push(Arg::Word(Word {
                utf8: Some(val.to_owned()),
                os: OsString::from(val),
            }));
        } else {
            for f in body.chars() {
                vec.push(Arg::Short(f));
            }
        }
    } else {
        vec.push(Arg::Word(Word {
            utf8: Some(utf8.to_string()),
            os,
        }));
    }
}

impl Args {
    /// Get a short or long flag: `-f` / `--flag`
    ///
    /// Returns false if value is not present
    pub(crate) fn take_flag<P>(&mut self, predicate: P) -> bool
    where
        P: Fn(&Arg) -> bool,
    {
        let mut iter = self.items_iter().skip_while(|i| !predicate(i.1));
        if let Some((ix, _)) = iter.next() {
            self.remove(ix);
            true
        } else {
            false
        }
    }

    /// get a short or long arguments
    ///
    /// Returns Ok(None) if flag is not present
    /// Returns Err if flag is present but value is either missing or strange.
    pub(crate) fn take_arg<P>(&mut self, predicate: P) -> Result<Option<Word>, Error>
    where
        P: Fn(&Arg) -> bool,
    {
        let mut iter = self.items_iter().skip_while(|i| !predicate(i.1));
        let (key_ix, arg) = match iter.next() {
            Some(v) => v,
            None => return Ok(None),
        };
        let (val_ix, val) = match iter.next() {
            Some((ix, Arg::Word(w))) => (ix, w),
            Some((_ix, flag)) => {
                return Err(Error::Stderr(format!(
                    "{arg} requires an argument, got flag {flag}"
                )))
            }
            _ => return Err(Error::Stderr(format!("{arg} requires an argument"))),
        };
        let val = val.clone();
        self.current = Some(val.clone());
        self.remove(key_ix);
        self.remove(val_ix);
        Ok(Some(val))
    }

    /// gets first positional argument present
    ///
    /// returns Ok(None) if imput is empty
    /// returns Err if first positional argument is a flag
    pub(crate) fn take_positional_word(&mut self) -> Result<Option<Word>, Error> {
        match self.items_iter().next() {
            Some((ix, Arg::Word(w))) => {
                let w = w.clone();
                self.current = Some(w.clone());
                self.remove(ix);
                Ok(Some(w))
            }
            Some((_, arg)) => Err(Error::Stderr(format!("Expected an argument, got {arg}"))),
            None => Ok(None),
        }
    }

    /// take a static string argument from the first present argument
    pub(crate) fn take_cmd(&mut self, word: &'static str) -> bool {
        if let Some((ix, Arg::Word(w))) = self.items_iter().next() {
            if w.utf8.as_ref().map_or(false, |ww| ww == word) {
                self.remove(ix);
                return true;
            }
        }
        false
    }

    pub(crate) fn peek(&self) -> Option<&Arg> {
        self.items_iter().next().map(|x| x.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn long_arg() {
        let mut a = Args::from(&["--speed", "12"]);
        let s = a.take_arg(|f| f.is_long("speed")).unwrap().unwrap();
        assert_eq!(s.utf8.unwrap(), "12");
        assert!(a.is_empty());
    }
    #[test]
    fn long_flag_and_positional() {
        let mut a = Args::from(&["--speed", "12"]);
        let flag = a.take_flag(|f| f.is_long("speed"));
        assert!(flag);
        assert!(!a.is_empty());
        let s = a.take_positional_word().unwrap().unwrap();
        assert_eq!(s.utf8.unwrap(), "12");
        assert!(a.is_empty());
    }

    #[test]
    fn multiple_short_flags() {
        let mut a = Args::from(&["-vvv"]);
        assert!(a.take_flag(|f| f.is_short('v')));
        assert!(a.take_flag(|f| f.is_short('v')));
        assert!(a.take_flag(|f| f.is_short('v')));
        assert!(!a.take_flag(|f| f.is_short('v')));
        assert!(a.is_empty());
    }

    #[test]
    fn long_arg_with_equality() {
        let mut a = Args::from(&["--speed=12"]);
        let s = a.take_arg(|f| f.is_long("speed")).unwrap().unwrap();
        assert_eq!(s.utf8.unwrap(), "12");
        assert!(a.is_empty());
    }

    #[test]
    fn long_arg_with_equality_and_minus() {
        let mut a = Args::from(&["--speed=-12"]);
        let s = a.take_arg(|f| f.is_long("speed")).unwrap().unwrap();
        assert_eq!(s.utf8.unwrap(), "-12");
        assert!(a.is_empty());
    }

    #[test]
    fn short_arg_with_equality() {
        let mut a = Args::from(&["-s=12"]);
        let s = a.take_arg(|f| f.is_short('s')).unwrap().unwrap();
        assert_eq!(s.utf8.unwrap(), "12");
        assert!(a.is_empty());
    }

    #[test]
    fn short_arg_with_equality_and_minus() {
        let mut a = Args::from(&["-s=-12"]);
        let s = a.take_arg(|f| f.is_short('s')).unwrap().unwrap();
        assert_eq!(s.utf8.unwrap(), "-12");
        assert!(a.is_empty());
    }

    #[test]
    fn short_arg_without_equality() {
        let mut a = Args::from(&["-s", "12"]);
        let s = a.take_arg(|f| f.is_short('s')).unwrap().unwrap();
        assert_eq!(s.utf8.unwrap(), "12");
        assert!(a.is_empty());
    }

    #[test]
    fn two_short_flags() {
        let mut a = Args::from(&["-s", "-v"]);
        assert!(a.take_flag(|f| f.is_short('s')));
        assert!(a.take_flag(|f| f.is_short('v')));
        assert!(a.is_empty());
    }

    #[test]
    fn two_short_flags2() {
        let mut a = Args::from(&["-s", "-v"]);
        assert!(a.take_flag(|f| f.is_short('v')));
        assert!(!a.take_flag(|f| f.is_short('v')));
        assert!(a.take_flag(|f| f.is_short('s')));
        assert!(!a.take_flag(|f| f.is_short('s')));
        assert!(a.is_empty());
    }

    #[test]
    fn command_with_flags() {
        let mut a = Args::from(&["cmd", "-s", "v"]);
        assert!(a.take_cmd("cmd"));
        let s = a.take_arg(|f| f.is_short('s')).unwrap().unwrap();
        assert_eq!(s.utf8.unwrap(), "v");
        assert!(a.is_empty());
    }

    #[test]
    fn command_and_positional() {
        let mut a = Args::from(&["cmd", "pos"]);
        assert!(a.take_cmd("cmd"));
        let w = a.take_positional_word().unwrap().unwrap();
        assert_eq!(w.utf8.unwrap(), "pos");
        assert!(a.is_empty());
    }

    #[test]
    fn positionals_after_double_dash() {
        let mut a = Args::from(&["-v", "--", "-x"]);
        assert!(a.take_flag(|f| f.is_short('v')));
        let w = a.take_positional_word().unwrap().unwrap();
        assert_eq!(w.utf8.unwrap(), "-x");
        assert!(a.is_empty());
    }

    #[test]
    fn positionals_after_double_dash2() {
        let mut a = Args::from(&["-v", "12", "--", "-x"]);
        let w = a.take_arg(|f| f.is_short('v')).unwrap().unwrap();
        assert_eq!(w.utf8.unwrap(), "12");
        let w = a.take_positional_word().unwrap().unwrap();
        assert_eq!(w.utf8.unwrap(), "-x");
        assert!(a.is_empty());
    }
}
