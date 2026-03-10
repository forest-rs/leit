fn main() {
    let high_score = 0.9;
    let mid_score = 0.5;
    let low_score = 0.1;
    
    println!("high_score = {}, mid_score = {}, low_score = {}", 
             high_score, mid_score, low_score);
    
    // Simulate the current Hit::cmp logic
    fn cmp_scores(a: f32, b: f32) -> &'static str {
        match a.total_cmp(&b) {
            core::cmp::Ordering::Equal => "Equal",
            core::cmp::Ordering::Less => "Less -> Greater",
            core::cmp::Ordering::Greater => "Greater -> Less",
        }
    }
    
    println!("high vs mid: {}", cmp_scores(high_score, mid_score));
    println!("mid vs low: {}", cmp_scores(mid_score, low_score));
    println!("high vs low: {}", cmp_scores(high_score, low_score));
}
