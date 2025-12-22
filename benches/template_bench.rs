//! Quick benchmark to verify template resolution performance

use nika::template::resolve_templates;
use nika::{ContextWriter, ExecutionContext};
use std::time::Instant;

fn main() {
    // Setup context with some data
    let mut ctx = ExecutionContext::new();
    ctx.set_output("task1", "Hello World".to_string());
    ctx.set_output("task2", "Some output".to_string());
    ctx.set_output("analyze", "Analysis results here".to_string());

    // Test templates of varying complexity
    let templates = vec![
        "Simple text with no templates",
        "Output from {{task1}}",
        "Multiple {{task1}} and {{task2}} references",
        "Complex: {{task1}} with ${input.file} and ${env.HOME}",
        "{{analyze}} {{task1}} {{task2}} ${input.path} ${env.USER} mixed content",
    ];

    println!("Template Resolution Performance Test");
    println!("====================================\n");

    // Warm up the cache
    for template in &templates {
        let _ = resolve_templates(template, &ctx);
    }

    // Benchmark each template
    for template in &templates {
        let iterations = 100_000;
        let start = Instant::now();

        for _ in 0..iterations {
            let _ = resolve_templates(template, &ctx);
        }

        let elapsed = start.elapsed();
        let per_op = elapsed / iterations;

        println!("Template: {:60}", format!("\"{}\"", template));
        println!("  Time for {} iterations: {:?}", iterations, elapsed);
        println!("  Per operation: {:?}\n", per_op);
    }

    // Test SmartString performance
    use nika::smart_string::SmartString;

    println!("SmartString vs String Performance");
    println!("==================================\n");

    let short_ids = vec!["task-1", "step-2", "analyze", "validate"];
    let iterations = 1_000_000;

    // SmartString creation
    let start = Instant::now();
    for _ in 0..iterations {
        for id in &short_ids {
            let _ = SmartString::from(*id);
        }
    }
    let smart_elapsed = start.elapsed();

    // String creation
    let start = Instant::now();
    for _ in 0..iterations {
        for id in &short_ids {
            let _ = String::from(*id);
        }
    }
    let string_elapsed = start.elapsed();

    println!(
        "Creating {} short IDs x {} iterations:",
        short_ids.len(),
        iterations
    );
    println!("  SmartString: {:?}", smart_elapsed);
    println!("  String:      {:?}", string_elapsed);
    println!(
        "  Speedup:     {:.2}x faster",
        string_elapsed.as_secs_f64() / smart_elapsed.as_secs_f64()
    );
}
