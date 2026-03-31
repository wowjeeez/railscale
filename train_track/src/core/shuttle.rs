use crate::core::turnout::Turnout;

pub struct ShuttleLink<A, B> {
    first: A,
    second: B,
}

impl<A, B> ShuttleLink<A, B> {
    pub fn new(first: A, second: B) -> Self {
        Self { first, second }
    }
}

impl<A, B> Turnout for ShuttleLink<A, B>
where
    A: Turnout,
    B: Turnout<Input = A::Output>,
{
    type Input = A::Input;
    type Output = B::Output;

    fn process(&self, input: Self::Input) -> Option<Self::Output> {
        let intermediate = self.first.process(input)?;
        self.second.process(intermediate)
    }
}
