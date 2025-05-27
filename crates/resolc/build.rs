fn main() {
    match git2::Repository::open("../..") {
        Ok(repo) => {
            let head = repo.head().expect("should have head");
            let commit = head.peel_to_commit().expect("should have commit");
            let id = &commit.id().to_string()[..7];
            println!("cargo:rustc-env=GIT_COMMIT_HASH={id}");
        }
        Err(_) => println!("cargo:rustc-env=GIT_COMMIT_HASH=unknown"),
    };
}
