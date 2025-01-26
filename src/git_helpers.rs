use git2::{BranchType, Cred, CredentialType, Repository};
use crate::command::CommandError;

pub fn find_branch_ref(repository: &Repository, branch: &str) -> Result<String, CommandError> {
    let branch_ref = repository.find_branch(&branch, BranchType::Local).map_err(|_| CommandError::BranchNotFound(branch.to_owned()))?;
    let branch_ref = branch_ref.into_reference();
    let branch_ref = branch_ref.name().ok_or_else(|| CommandError::InternalError("Failed to unwrap branch ref".to_owned()))?;
    Ok(branch_ref.to_string())
}

pub fn create_ssh_credentials() -> impl FnMut(&str, Option<&str>, CredentialType) -> Result<Cred, git2::Error> {
    |_url, username_from_url, _allowed_types| {
        let username = username_from_url
            .ok_or_else(|| git2::Error::new(
                git2::ErrorCode::Auth,
                git2::ErrorClass::Ssh,
                &"Failed to get username for SSH"
            ))?;

        Cred::ssh_key_from_agent(username)
    }
}

pub fn merge<'a>(
    repository: &'a Repository,
    remote_branch: &str,
    fetch_commit: git2::AnnotatedCommit<'a>
) -> Result<(), git2::Error> {
    // 1. do a merge analysis
    let analysis = repository.merge_analysis(&[&fetch_commit])?;

    // 2. Do the appropriate merge
    if analysis.0.is_fast_forward() {
        // do a fast forward
        let ref_name = format!("refs/heads/{}", remote_branch);
        match repository.find_reference(&ref_name) {
            Ok(mut r) => {
                fast_forward(repository, &mut r, &fetch_commit)?;
            }
            Err(_) => {
                // The branch doesn't exist so just set the reference to the
                // commit directly. Usually this is because you are pulling
                // into an empty repository.
                repository.reference(
                    &ref_name,
                    fetch_commit.id(),
                    true,
                    &format!("Setting {} to {}", remote_branch, fetch_commit.id()),
                )?;
                repository.set_head(&ref_name)?;
                repository.checkout_head(Some(
                    git2::build::CheckoutBuilder::default()
                        .allow_conflicts(true)
                        .conflict_style_merge(true)
                        .force(),
                ))?;
            }
        };
    } else if analysis.0.is_normal() {
        // do a normal merge
        let head_commit = repository.reference_to_annotated_commit(&repository.head()?)?;
        normal_merge(&repository, &head_commit, &fetch_commit)?;
    }

    Ok(())
}

fn fast_forward(
    repository: &Repository,
    lb: &mut git2::Reference,
    rc: &git2::AnnotatedCommit
) -> Result<(), git2::Error> {
    let name = match lb.name() {
        Some(s) => s.to_string(),
        None => String::from_utf8_lossy(lb.name_bytes()).to_string(),
    };

    let msg = format!("Fast-Forward: Setting {} to id: {}", name, rc.id());
    lb.set_target(rc.id(), &msg)?;
    repository.set_head(&name)?;
    repository.checkout_head(Some(
        git2::build::CheckoutBuilder::default()
            // For some reason the force is required to make the working directory actually get updated
            // I suspect we should be adding some logic to handle dirty working directory states
            // but this is just an example so maybe not.
            .force(),
    ))?;
    Ok(())
}

fn normal_merge(
    repository: &Repository,
    local: &git2::AnnotatedCommit,
    remote: &git2::AnnotatedCommit
) -> Result<(), git2::Error> {
    let local_tree = repository.find_commit(local.id())?.tree()?;
    let remote_tree = repository.find_commit(remote.id())?.tree()?;
    let ancestor = repository
        .find_commit(repository.merge_base(local.id(), remote.id())?)?
        .tree()?;
    let mut idx = repository.merge_trees(&ancestor, &local_tree, &remote_tree, None)?;

    if idx.has_conflicts() {
        println!("Merge conflicts detected...");
        repository.checkout_index(Some(&mut idx), None)?;
        return Ok(());
    }

    let result_tree = repository.find_tree(idx.write_tree_to(repository)?)?;
    // now create the merge commit
    let msg = format!("Merge: {} into {}", remote.id(), local.id());
    let sig = repository.signature()?;
    let local_commit = repository.find_commit(local.id())?;
    let remote_commit = repository.find_commit(remote.id())?;

    // Do our merge commit and set current branch head to that commit.
    let _merge_commit = repository.commit(
        Some("HEAD"),
        &sig,
        &sig,
        &msg,
        &result_tree,
        &[&local_commit, &remote_commit],
    )?;

    // Set working tree to match head.
    repository.checkout_head(None)?;
    Ok(())
}