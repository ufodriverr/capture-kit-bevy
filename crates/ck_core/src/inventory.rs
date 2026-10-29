//! Item containers: the player's bag, weapon slots, chests. Every change returns what actually happened, so
//! nothing is created or lost silently.

use serde::{Deserialize, Serialize};

/// One slot's content. `durability` is set for items that have it (weapons, tools).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stack {
    pub item: String,
    pub count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub durability: Option<f32>,
}

impl Stack {
    pub fn new(item: &str, count: u32) -> Self {
        Self { item: item.to_owned(), count, durability: None }
    }
}

/// What the container needs to know about an item.
pub trait ItemInfo {
    fn max_stack(&self, item: &str) -> u32;
    fn weight(&self, item: &str) -> f32;
    /// Starting durability for a new item, if it has any.
    fn durability(&self, item: &str) -> Option<f32>;
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Container {
    pub slots: Vec<Option<Stack>>,
}

impl Container {
    pub fn new(slots: usize) -> Self {
        Self { slots: vec![None; slots] }
    }

    pub fn count(&self, item: &str) -> u32 {
        self.slots.iter().flatten().filter(|s| s.item == item).map(|s| s.count).sum()
    }

    pub fn weight(&self, info: &dyn ItemInfo) -> f32 {
        self.slots.iter().flatten().map(|s| info.weight(&s.item) * s.count as f32).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.iter().all(Option::is_none)
    }

    /// Adds up to `count`; returns how many did not fit.
    pub fn add(&mut self, item: &str, count: u32, info: &dyn ItemInfo) -> u32 {
        let max = info.max_stack(item).max(1);
        let mut left = count;
        if info.durability(item).is_none() {
            for s in self.slots.iter_mut().flatten().filter(|s| s.item == item) {
                let room = max.saturating_sub(s.count);
                let n = room.min(left);
                s.count += n;
                left -= n;
                if left == 0 {
                    return 0;
                }
            }
        }
        for slot in self.slots.iter_mut().filter(|s| s.is_none()) {
            let n = max.min(left);
            *slot = Some(Stack { item: item.to_owned(), count: n, durability: info.durability(item) });
            left -= n;
            if left == 0 {
                return 0;
            }
        }
        left
    }

    /// Puts a whole stack (keeping its durability) into a free slot or merges it; returns what didn't fit.
    pub fn add_stack(&mut self, stack: Stack, info: &dyn ItemInfo) -> Option<Stack> {
        if stack.durability.is_none() {
            let left = self.add(&stack.item, stack.count, info);
            return (left > 0).then_some(Stack { count: left, ..stack });
        }
        match self.slots.iter_mut().find(|s| s.is_none()) {
            Some(slot) => {
                *slot = Some(stack);
                None
            }
            None => Some(stack),
        }
    }

    /// How many of `count` would fit.
    pub fn room_for(&self, item: &str, count: u32, info: &dyn ItemInfo) -> u32 {
        let mut c = self.clone();
        count - c.add(item, count, info)
    }

    /// Removes exactly `count` or nothing; returns whether it did.
    pub fn remove(&mut self, item: &str, count: u32) -> bool {
        if self.count(item) < count {
            return false;
        }
        let mut left = count;
        for slot in self.slots.iter_mut().rev() {
            if let Some(s) = slot
                && s.item == item
            {
                let n = s.count.min(left);
                s.count -= n;
                left -= n;
                if s.count == 0 {
                    *slot = None;
                }
                if left == 0 {
                    break;
                }
            }
        }
        true
    }

    /// Takes the stack out of a slot.
    pub fn take(&mut self, slot: usize) -> Option<Stack> {
        self.slots.get_mut(slot).and_then(Option::take)
    }

    /// Sorts by item id and merges stacks (the bag's sort button).
    pub fn sort(&mut self, info: &dyn ItemInfo) {
        let mut all: Vec<Stack> = self.slots.iter_mut().filter_map(Option::take).collect();
        all.sort_by(|a, b| a.item.cmp(&b.item));
        for s in all {
            let _ = self.add_stack(s, info);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Info;
    impl ItemInfo for Info {
        fn max_stack(&self, item: &str) -> u32 {
            if item == "Bat" { 1 } else { 9999 }
        }
        fn weight(&self, item: &str) -> f32 {
            if item == "Wood" { 3.0 } else { 1.0 }
        }
        fn durability(&self, item: &str) -> Option<f32> {
            (item == "Bat").then_some(150.0)
        }
    }

    #[test]
    fn add_remove_keeps_counts() {
        let mut c = Container::new(3);
        assert_eq!(c.add("Wood", 10, &Info), 0);
        assert_eq!(c.add("Wood", 5, &Info), 0);
        assert_eq!(c.count("Wood"), 15);
        assert_eq!(c.slots.iter().flatten().count(), 1);
        assert!(!c.remove("Wood", 16));
        assert_eq!(c.count("Wood"), 15, "failed remove takes nothing");
        assert!(c.remove("Wood", 15));
        assert!(c.is_empty());
    }

    #[test]
    fn full_bag_returns_leftover() {
        let mut c = Container::new(2);
        assert_eq!(c.add("Bat", 3, &Info), 1);
        assert_eq!(c.room_for("Wood", 4, &Info), 0);
        assert_eq!(c.weight(&Info), 2.0);
        assert_eq!(c.slots[0].as_ref().unwrap().durability, Some(150.0));
    }
}
