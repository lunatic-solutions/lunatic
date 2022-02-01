use std::{ vec::Vec, sync::Mutex };
use lazy_static::lazy_static;
use testanything::{ tap_suite_builder::TapSuiteBuilder, tap_test_builder::TapTestBuilder };
use crate::state::HashMapId;

#[derive(Clone, Debug)]
pub struct TestNode {
    name: String,
    children: Vec<u64>,
    ok: bool,
    comments: String,
}

impl TestNode {

    pub fn generate_tap(&self, map: &HashMapId<TestNode>, builder: &TapSuiteBuilder) -> () {
        let suite = TapTestBuilder::new()
            .name(self.name.as_str())
            .passed(self.ok);
        let comments: Vec<&str> = self.comments.split("\r\n")
            .into_iter()
            .collect();
        let tap = suite
            .diagnostics(comments.as_slice())
            .finalize();

        builder.tests(vec![tap]);
        for child in self.children {
            // for each child, obtain the child, and call generate_tap on it
        }
    }

    pub fn new(name: &[u8]) -> TestNode {
        TestNode {
            name: String::from_utf8_lossy(name).into_owned(),
            children: Vec::new(),
            ok: false,
            comments: String::new(),
        }
    }

    //% Push a child to this test node by it's id
    pub fn push_child(&mut self, child_id: u64) -> () {
        self.children.push(child_id);
    }

    pub fn ok(&mut self) -> () {
        self.ok = true;
    }
}

lazy_static!{
    pub static ref TESTS: Mutex<HashMapId<TestNode>> = {
        let mut hashmap = HashMapId::new();
        hashmap.add(TestNode::new(b""));
        Mutex::new(hashmap)
    };
}
