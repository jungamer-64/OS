//! Testing utilities for userland programs
//!
//! This module provides testing helpers and assertion macros

use crate::io::{println, eprintln};
use crate::process::exit;

/// Test result
pub type TestResult = Result<(), &'static str>;

/// Run a test function
pub fn run_test(name: &str, test_fn: fn() -> TestResult) {
    println!("[TEST] Running: {}", name);
    match test_fn() {
        Ok(()) => println!("[PASS] {}", name),
        Err(msg) => {
            eprintln!("[FAIL] {}: {}", name, msg);
            exit(1);
        }
    }
}

/// Assert that a condition is true
#[macro_export]
macro_rules! assert {
    ($cond:expr) => {
        if !$cond {
            return Err(concat!("Assertion failed: ", stringify!($cond)));
        }
    };
    ($cond:expr, $msg:expr) => {
        if !$cond {
            return Err($msg);
        }
    };
}

/// Assert that two values are equal
#[macro_export]
macro_rules! assert_eq {
    ($left:expr, $right:expr) => {
        match (&$left, &$right) {
            (left_val, right_val) => {
                if !(left_val == right_val) {
                    return Err(concat!(
                        "Assertion failed: ",
                        stringify!($left),
                        " != ",
                        stringify!($right)
                    ));
                }
            }
        }
    };
}

/// Assert that two values are not equal
#[macro_export]
macro_rules! assert_ne {
    ($left:expr, $right:expr) => {
        match (&$left, &$right) {
            (left_val, right_val) => {
                if left_val == right_val {
                    return Err(concat!(
                        "Assertion failed: ",
                        stringify!($left),
                        " == ",
                        stringify!($right)
                    ));
                }
            }
        }
    };
}

/// Test runner - runs multiple tests
pub struct TestRunner {
    passed: usize,
    failed: usize,
}

impl TestRunner {
    /// Create a new test runner
    pub fn new() -> Self {
        Self {
            passed: 0,
            failed: 0,
        }
    }
    
    /// Run a test
    pub fn test(&mut self, name: &str, test_fn: fn() -> TestResult) {
        println!("[TEST] Running: {}", name);
        match test_fn() {
            Ok(()) => {
                println!("[PASS] {}", name);
                self.passed += 1;
            }
            Err(msg) => {
                eprintln!("[FAIL] {}: {}", name, msg);
                self.failed += 1;
            }
        }
    }
    
    /// Print summary and exit with appropriate code
    pub fn finish(self) -> ! {
        println!("\n=== Test Summary ===");
        println!("Passed: {}", self.passed);
        println!("Failed: {}", self.failed);
        println!("Total:  {}", self.passed + self.failed);
        
        if self.failed > 0 {
            exit(1);
        } else {
            exit(0);
        }
    }
}
