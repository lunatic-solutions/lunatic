use std::{ vec::Vec, sync::Mutex, str };
use lazy_static::lazy_static;
use testanything::{ tap_test_builder::TapTestBuilder, tap_test::TapTest };
use crate::state::HashMapId;

#[derive(Clone, Debug)]
pub struct TestNode {
    name: String,
    children: Vec<u64>,
    ok: bool,
    negated: bool,
    comments: String,
}

impl TestNode {

    pub fn generate_tap(&self, map: &HashMapId<TestNode>, builder: &mut Vec<TapTest>) -> () {
        let suite = TapTestBuilder::new()
            .name(self.name.as_str())
            // the test passes if it's okay xor if it's negated
            .passed(self.ok ^ self.negated)
            .diagnostics(
                self.comments.split("\r\n")
                .into_iter()
                .collect::<Vec<&str>>()
                .as_slice()
            )
            .finalize();

        builder.push(suite);

        for child in &self.children {
            // for each child, obtain the child, and call generate_tap on it
            let child = map.get(*child)
                .expect("Cannot find test child.");
            
            child.generate_tap(map, builder);
        }
    }

    pub fn add_comment(&mut self, comment: &[u8]) -> () {
        self.comments.push_str(String::from_utf8_lossy(comment).into_owned().as_str());
    }

    pub fn new(name: &[u8], negated: bool) -> TestNode {
        TestNode {
            name: String::from_utf8_lossy(name).into_owned(),
            children: Vec::new(),
            // if the test is negated, we assume it's okay
            ok: negated,
            negated: negated,
            comments: String::new(),
        }
    }

    //% Push a child to this test node by it's id
    pub fn push_child(&mut self, child_id: u64) -> () {
        self.children.push(child_id);
    }

    pub fn ok(&mut self) -> () {
        // if the test is negated, it's not okay
        self.ok = !self.negated;
    }
}

lazy_static!{
    pub static ref TESTS: Mutex<HashMapId<TestNode>> = {
        let mut hashmap = HashMapId::new();
        hashmap.add(TestNode::new(b"", false));
        Mutex::new(hashmap)
    };
}
