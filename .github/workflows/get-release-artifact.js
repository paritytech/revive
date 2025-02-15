module.exports = async ({
  octokit,
  context,
  releasePrefix,
  artifactSuffix,
}) => {
  let page = 1;

  while (true) {
    const res = await octokit.rest.repos.listReleases({
      owner: context.repo.owner,
      repo: context.repo.repo,
      per_page: 100,
      page,
    });
    if (res.data.length === 0) {
      throw new Error(
        `No LLVM releases with '${artifactSuffix}' atifacts found! Please release LLVM before running this workflow.`,
      );
    }

    for (let release of res.data) {
      if (release.tag_name.startsWith(releasePrefix)) {
        for (let asset of release.assets) {
          if (asset.name.includes(artifactSuffix)) {
            return asset.browser_download_url;
          }
        }
        console.warn(
          `LLVM release ${release.tag_name} doesn't have a '${artifactSuffix}' artifact; searching for older releases...`,
        );
      }
    }
    page++;
  }
};
