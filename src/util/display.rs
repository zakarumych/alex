use core::fmt::{self, Display};

pub struct DisplayPunctuatedWrapper<T>(pub T);

impl<T> Display for DisplayPunctuatedWrapper<T>
where
    T: DisplayPunctuated,
{
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(fmt)
    }
}

pub trait DisplayPunctuated {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result;

    fn display_punctuated(self) -> DisplayPunctuatedWrapper<Self>
    where
        Self: Sized,
    {
        DisplayPunctuatedWrapper(self)
    }
}

impl<I> DisplayPunctuated for I
where
    I: IntoIterator + Clone,
    I::Item: Display,
{
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut iter = self.clone().into_iter();

        if let Some(head) = iter.next() {
            head.fmt(fmt)?;
            for elem in iter {
                write!(fmt, ", {}", elem)?;
            }
        }
        Ok(())
    }
}
