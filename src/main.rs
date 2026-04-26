use clipper2::*;

fn main() {
    println!("Slicer Engine - Hello World!");
    println!("Version: {}", env!("CARGO_PKG_VERSION"));
    println!();
    
    // Basic Clipper2 example
    let mut subjects: Paths<Centi> = Paths::default();
    let mut subject: Path<Centi> = Path::default();
    subject.push(Point::new(0.0, 0.0));
    subject.push(Point::new(1000.0, 0.0));
    subject.push(Point::new(1000.0, 1000.0));
    subject.push(Point::new(0.0, 1000.0));
    subjects.push(subject);
    
    println!("Successfully initialized Clipper2");
    println!("Subject path created with 4 vertices");
    println!("Ready for slicing operations");
}
