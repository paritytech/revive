fn main() {
    let repo = git2::Repository::open("../..").expect("should be a repository");
    let head = repo.head().expect("should have head");
    let commit = head.peel_to_commit().expect("should have commit");
    let id = &commit.id().to_string()[..7];
    println!("cargo:rustc-env=GIT_COMMIT_HASH={id}");
}
