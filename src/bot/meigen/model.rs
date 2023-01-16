use {
    serde::{Deserialize, Serialize},
    std::str::FromStr,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MeigenId(pub u32);

impl std::fmt::Display for MeigenId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for MeigenId {
    type Err = <u32 as FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(MeigenId(s.parse()?))
    }
}

impl MeigenId {
    pub fn succ(self) -> Self {
        Self(self.0 + 1)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meigen {
    pub id: MeigenId,
    pub author: String,
    pub content: String,
    pub loved_user_id: Vec<u64>,
}

impl Meigen {
    pub fn loves(&self) -> usize {
        self.loved_user_id.len()
    }

    #[allow(dead_code)]
    pub fn is_loving(&self, user_id: u64) -> bool {
        self.loved_user_id.contains(&user_id)
    }
}

impl std::fmt::Display for Meigen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let loves = self.loves();
        let loves_description = if loves > 0 {
            format!("(♥ x{loves})")
        } else {
            String::new()
        };

        write!(
            f,
            "Meigen No.{} {}
```
{}
    --- {}
```",
            self.id, loves_description, self.content, self.author
        )
    }
}
