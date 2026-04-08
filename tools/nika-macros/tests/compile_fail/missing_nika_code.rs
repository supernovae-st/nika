use nika_macros::NikaErrorCode;

#[derive(NikaErrorCode)]
enum Bad {
    MissingCode,
}

fn main() {}
