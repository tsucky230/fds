# GitHub Pages Setup Guide for FDS

This guide explains how to enable GitHub Pages for the FDS repository.

## Quick Setup

### 1. Enable GitHub Pages

1. Go to **Settings** > **Pages** on your GitHub repository
2. Under "Build and deployment":
   - **Source**: Select "Deploy from a branch"
   - **Branch**: Select `main`
   - **Folder**: Select `/docs`
3. Click **Save**

Your site will be available at: `https://tsucky230.github.io/fds`

### 2. Verify the Setup

After enabling, GitHub Actions will automatically build and deploy your site. Check the following:

- **Actions tab** — Look for "pages build and deployment" workflow
- **Pages settings** — Should show your site URL and deployment status
- **Site URL** — Visit `https://tsucky230.github.io/fds` to see your site

### 3. File Structure

The site is built from the `/docs` folder:

```
docs/
├── _config.yml     # Jekyll theme & metadata
├── index.md        # Homepage (you are here)
└── .gitkeep        # Ensures folder is tracked
```

## 🎨 Customization

### Change Theme

In `docs/_config.yml`, update:

```yaml
theme: jekyll-theme-minimal  # Options: minimal, cayman, slate, etc.
```

Other popular themes:
- `jekyll-theme-cayman` — Modern with header
- `jekyll-theme-slate` — Dark theme
- `jekyll-theme-architect` — Clean layout

### Add More Pages

Create new markdown files in `/docs/`:

```markdown
# docs/guide.md
# User Guide

[Your content here]
```

Then link from `index.md`:

```markdown
- [User Guide](guide.md)
```

### Custom Domain (Optional)

To use a custom domain:

1. In `docs`, create a `CNAME` file:
   ```
   yourdomain.com
   ```

2. Update your DNS records to point to GitHub Pages
3. In Settings > Pages, enter your domain

## 📝 Content Tips

- Use Markdown for all content
- GitHub automatically converts `.md` to HTML
- Front matter (YAML) is optional but useful:
  ```yaml
  ---
  title: My Page
  layout: default
  ---
  ```

## 🔗 Useful Links

- [GitHub Pages Documentation](https://docs.github.com/en/pages)
- [Jekyll Documentation](https://jekyllrb.com/docs/)
- [Markdown Guide](https://www.markdownguide.org/)

## Troubleshooting

**Site not appearing?**
- Wait a few minutes for GitHub Actions to complete
- Check Actions tab for build errors
- Ensure `_config.yml` is valid YAML

**Changes not updating?**
- Give GitHub Actions time to rebuild (1-2 minutes)
- Check that changes are pushed to the `main` branch

---

For more info, see [GitHub Pages docs](https://docs.github.com/en/pages).
