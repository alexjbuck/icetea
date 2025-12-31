# Security Audit Report: IceTea TUI

**Date:** 2025-12-31
**Auditor:** Automated Security Analysis
**Version:** 0.1.0
**Codebase:** IceTea - Terminal User Interface for Apache Iceberg Catalogs
**Application Type:** Command-Line Interface (CLI) Tool

---

## Executive Summary

This security audit evaluates the IceTea TUI application through the lens of a **command-line tool** where users already have shell access to the host machine. This significantly changes the threat model compared to web applications or network services.

**Key Findings:**
- **Overall Risk: LOW** - Appropriate security posture for a CLI tool
- Most identified issues are **robustness concerns** rather than security vulnerabilities
- Follows standard CLI tool patterns (similar to aws-cli, kubectl, psql, etc.)
- Primary concern: Preventing accidental credential commits to version control

**Critical Action Items:**
1. Add `icetea.toml` to `.gitignore` (MEDIUM severity)
2. Monitor unmaintained dependencies (LOW severity)
3. Add input limits for robustness (LOW severity)

**Overall Risk Assessment: LOW - Ready for development/internal use with minor improvements**

---

## Threat Model: CLI Application

### Security Context

**Key Assumption:** Users running IceTea already have:
- Shell access to the host system
- File system read/write permissions in their home directory
- Ability to read configuration files (including credentials)
- Access to execute arbitrary commands
- Network access to catalog endpoints

**Attack Surface:**
- Local only (not exposed to network)
- Single user per instance
- Credentials protected by OS file permissions
- No privilege escalation (runs with user's permissions)

**Comparison to Similar Tools:**
- AWS CLI (`~/.aws/credentials`) - plaintext credentials
- Kubernetes (`~/.kube/config`) - plaintext certificates and tokens
- PostgreSQL (`~/.pgpass`) - plaintext passwords
- Git (`~/.gitconfig`, `~/.git-credentials`) - plaintext tokens
- Docker (`~/.docker/config.json`) - plaintext registry auth

IceTea follows the same security model as these established tools.

---

## Revised Vulnerability Assessment

### 1. Missing .gitignore Entry for Configuration (MEDIUM)

**Location:** `.gitignore`
**Severity:** MEDIUM (unchanged - this is the primary concern)

**Description:**
The `.gitignore` file does not exclude `icetea.toml`, creating risk of accidentally committing credentials to version control.

**Why This Matters for CLI Tools:**
Even though plaintext config files are standard for CLI tools, they should **never** be committed to git. This is a universal best practice across all CLI tooling.

**Impact:**
- Credentials exposed in git history
- Credentials shared across team/public repositories
- Difficult to rotate compromised credentials

**Examples from Other Tools:**
```bash
# AWS CLI
.gitignore: .aws/

# Kubernetes
.gitignore: .kube/config

# Git itself
.gitignore: .git-credentials
```

**Recommendation:**
Add to `.gitignore`:
```gitignore
# Configuration files with secrets
icetea.toml
*.toml
!icetea.toml.example

# Environment files
.env
.env.*
!.env.example
```

**Status:** ✅ Should fix before first release

---

### 2. Unmaintained Dependencies (LOW)

**Detected by:** `cargo audit`
**Severity:** LOW (downgraded from MEDIUM)

**Findings:**

#### paste v1.0.15 (RUSTSEC-2024-0436)
- **Status:** Unmaintained (as of 2024-10-07)
- **Source:** Transitive dependency via `parquet` crate
- **Actual Risk:** Very low - proc-macro crate with limited attack surface

#### rustls-pemfile v2.2.0 (RUSTSEC-2025-0134)
- **Status:** Unmaintained (as of 2025-11-28)
- **Source:** Transitive dependency via `object_store` crate
- **Actual Risk:** Low - certificate parsing library

**Why Low Severity:**
- No known CVEs in either dependency
- Transitive dependencies (not direct)
- Upstream crates (`parquet`, `object_store`) will likely update
- CLI tool has limited exposure compared to network services

**Recommendation:**
- Run `cargo audit` periodically (monthly)
- Update dependencies with `cargo update` regularly
- Monitor upstream crates for updates

**Status:** ⚠️ Monitor but not blocking

---

### 3. Query Input Without Validation (LOW)

**Location:** `src/iceberg/query.rs:33-44`
**Severity:** LOW (downgraded from CRITICAL)

**Description:**
User input from the query interface is passed directly to DataFusion's SQL engine without validation.

**Original Code:**
```rust
pub async fn execute_query(&self, sql: &str) -> Result<QueryResults> {
    let df = self.ctx.sql(sql).await
        .context("Failed to parse SQL query")?;
```

**Why This Is Not a Security Issue for CLI Tools:**

1. **User Already Has Credentials:** A user with shell access can read `icetea.toml` and use credentials directly:
   ```bash
   cat ~/.config/icetea/icetea.toml  # Read credentials
   curl -H "Authorization: Bearer $TOKEN" https://catalog/api  # Use directly
   ```

2. **No Privilege Escalation:** The query runs with the same permissions as the user's configured catalogs. Cannot access data the user shouldn't have.

3. **Standard CLI Pattern:** Similar to:
   - `psql -c "DROP DATABASE prod"` - PostgreSQL CLI
   - `mysql -e "DELETE FROM users"` - MySQL CLI
   - `aws dynamodb delete-table --table-name prod` - AWS CLI

   All of these trust the user not to shoot themselves in the foot.

**What This Actually Prevents:**
- ❌ Not protecting against malicious users (they already have access)
- ✅ Protecting against accidental mistakes
- ✅ Providing better error messages
- ✅ Enforcing organizational policies (e.g., read-only mode)

**Recommendations for Robustness (not security):**
1. Add optional read-only mode flag: `icetea --read-only`
2. Add query confirmation for destructive operations (DROP, DELETE, etc.)
3. Add query timeout to prevent runaway queries
4. Consider query logging for audit purposes

**Example Improvement:**
```rust
pub async fn execute_query(&self, sql: &str, options: &QueryOptions) -> Result<QueryResults> {
    // Optional: Warn on destructive queries
    if options.interactive && is_destructive_query(sql) {
        confirm_query(sql)?;
    }

    // Optional: Enforce read-only mode
    if options.read_only && is_write_query(sql) {
        bail!("Write queries not allowed in read-only mode");
    }

    let df = self.ctx.sql(sql).await
        .context("Failed to parse SQL query")?;
    ...
}
```

**Status:** ⚠️ Nice-to-have for usability, not critical

---

### 4. Plaintext Credential Storage (NOT A VULNERABILITY)

**Location:** `src/config.rs`, `icetea.toml.example`
**Severity:** INFORMATIONAL (downgraded from CRITICAL)

**Description:**
Authentication tokens and AWS credentials are stored in plaintext in `icetea.toml`.

**Why This Is Standard Practice for CLI Tools:**

CLI tools universally use plaintext credential storage protected by file system permissions:

```bash
# AWS CLI - plaintext credentials
$ cat ~/.aws/credentials
[default]
aws_access_key_id = AKIAIOSFODNN7EXAMPLE
aws_secret_access_key = wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY

# Kubernetes - plaintext certificates and tokens
$ cat ~/.kube/config
users:
- name: cluster-admin
  user:
    client-certificate-data: LS0tLS1CRUdJTi...
    token: eyJhbGciOiJSUzI1NiIsImtpZCI6IiJ9...

# PostgreSQL - plaintext passwords
$ cat ~/.pgpass
localhost:5432:mydb:postgres:secretpassword

# Docker - plaintext registry auth
$ cat ~/.docker/config.json
{
  "auths": {
    "https://index.docker.io/v1/": {
      "auth": "dXNlcm5hbWU6cGFzc3dvcmQ="
    }
  }
}
```

**Security Model:**
- Protection via OS file permissions (chmod 600 or 644)
- User's responsibility to secure their home directory
- System administrator's responsibility to configure proper user isolation
- No encryption provides any real security if attacker has shell access

**Best Practices (Already Followed):**
- ✅ Store credentials in user's config directory
- ✅ Document where credentials are stored
- ✅ Provide example configuration file
- ✅ Support environment variables as alternative

**Optional Enhancements (Not Required):**
- Support environment variables for all credentials
- Add warning message on first run about credential storage
- Document file permission recommendations in README

**Status:** ✅ No changes required - standard CLI practice

---

### 5. Path Traversal in Config Loading (INFORMATIONAL)

**Location:** `src/config.rs:109-111`, `src/cli.rs:11`
**Severity:** INFORMATIONAL (downgraded from HIGH)

**Description:**
Configuration file paths from CLI arguments are used without validation.

**Vulnerable Code:**
```rust
#[arg(short, long, value_name = "FILE", env = "ICETEA_CONFIG")]
pub config: Option<PathBuf>,

if let Some(path) = config_path {
    figment = figment.merge(Toml::file(path));
}
```

**Why This Is Not a Security Issue:**

User with shell access can already read any file they have permissions for:
```bash
# User can already do this:
cat /etc/passwd
cat /path/to/any/file

# So this doesn't grant new capabilities:
icetea --config /etc/passwd  # Will fail to parse as TOML
```

**What This Actually Is:**
- A robustness/user experience issue
- Can provide confusing error messages
- User might accidentally specify wrong file

**Recommendations for Better UX:**
```rust
fn load_config(path: &PathBuf) -> Result<Config> {
    // Provide helpful error messages
    if !path.exists() {
        bail!("Config file not found: {}", path.display());
    }

    if !path.is_file() {
        bail!("Config path is not a file: {}", path.display());
    }

    // Check file size to prevent accidentally loading huge files
    let metadata = path.metadata()?;
    if metadata.len() > 10 * 1024 * 1024 {  // 10MB
        bail!("Config file too large: {} bytes", metadata.len());
    }

    // Try to load and provide clear error on parse failure
    figment.merge(Toml::file(path))
        .extract()
        .with_context(|| format!("Failed to parse config file: {}", path.display()))
}
```

**Status:** ⚠️ Nice-to-have for better error messages

---

### 6. Unlimited Input Buffers (LOW)

**Location:** `src/app.rs:28, 129`
**Severity:** LOW (downgraded from HIGH)

**Description:**
The query input buffer has no size limits.

**Vulnerable Code:**
```rust
pub query_input: String,  // No size limit

KeyCode::Char(c) => {
    self.query_input.push(c);  // Unbounded growth
}
```

**Why Low Severity:**

1. **Local DoS Only:** Only affects the user running the tool
2. **User Can Already DoS:** Many other ways to exhaust memory:
   ```bash
   yes | head -n 999999999 > /tmp/bigfile  # Fill disk
   :(){ :|:& };:  # Fork bomb
   stress --vm 1 --vm-bytes 10G  # Memory exhaustion
   ```

3. **Self-Inflicted:** User would have to intentionally hold down a key

**Why Still Worth Fixing:**

This is a **robustness** issue, not security:
- Prevents accidental paste of huge files
- Improves user experience
- Prevents crashes from extremely long queries

**Recommendation:**
```rust
const MAX_QUERY_LENGTH: usize = 100_000;  // 100KB should be plenty

KeyCode::Char(c) => {
    if self.query_input.len() < MAX_QUERY_LENGTH {
        self.query_input.push(c);
    } else {
        // Optional: Show warning in status bar
        self.status_message = Some("Query too long - limit reached".to_string());
    }
}
```

**Status:** ⚠️ Good practice for robustness

---

### 7. Cursor Position Integer Overflow (LOW)

**Location:** `src/ui/query_input.rs:35-42`
**Severity:** LOW (unchanged)

**Description:**
Cursor position calculation could overflow with very long input.

**Code:**
```rust
frame.set_cursor_position((
    area.x + app.query_input.len() as u16 + 1,  // Could overflow
    area.y + 1,
));
```

**Why Low Severity:**
- Requires query length > 65,535 characters
- Only affects UI rendering
- Fixed by query length limit above

**Recommendation:**
```rust
let cursor_x = area.x.saturating_add(
    app.query_input.len().min((u16::MAX - area.x - 1) as usize) as u16
).saturating_add(1);
frame.set_cursor_position((cursor_x, area.y + 1));
```

**Status:** ⚠️ Fix alongside query length limit

---

### 8. Information Disclosure via Error Messages (NOT APPLICABLE)

**Location:** `src/iceberg/catalog.rs:88-100`
**Severity:** INFORMATIONAL (downgraded from MEDIUM)

**Description:**
Error messages include detailed information about catalog names, namespaces, and internal errors.

**Why This Is Fine for CLI Tools:**

1. **Verbose Errors Are Good:** CLI tools should be verbose to help users debug:
   ```bash
   $ aws s3 ls s3://nonexistent-bucket
   An error occurred (NoSuchBucket) when calling the ListObjectsV2 operation: The specified bucket does not exist

   $ kubectl get pod nonexistent
   Error from server (NotFound): pods "nonexistent" not found
   ```

2. **User Already Has Access:** The user can query this information directly through the catalog API

3. **Debugging is Important:** Detailed errors are crucial for troubleshooting

**Recommendation:**
- ✅ Keep detailed error messages
- ✅ Consider adding `--verbose` flag for even more detail
- ✅ Include stack traces in verbose mode

**Status:** ✅ Current behavior is correct for CLI tools

---

## Non-Issues (Removed from Original Report)

The following items from the original audit are **not applicable** to CLI tools:

### ❌ Rate Limiting
- **Why Not Applicable:** User can just run the tool multiple times
- No benefit for local CLI tools

### ❌ Authentication on TUI Access
- **Why Not Applicable:** OS handles authentication (user login)
- Adding password would just annoy users

### ❌ Session Timeouts
- **Why Not Applicable:** CLI tools don't maintain sessions
- Each execution is independent

### ❌ Network Security Enforcement
- **Why Not Applicable:** User controls their own network config
- Should support both HTTP and HTTPS based on user needs

---

## Actual Security Recommendations for CLI Tools

### Immediate Actions

1. **Add Configuration Files to .gitignore** (MEDIUM Priority)
   ```gitignore
   # Add to .gitignore
   icetea.toml
   *.toml
   !icetea.toml.example
   .env
   .env.*
   !.env.example
   ```

2. **Document Credential Storage** (LOW Priority)
   Add to README.md:
   ```markdown
   ## Security

   IceTea stores credentials in `icetea.toml` in plaintext, similar to
   AWS CLI, kubectl, and other CLI tools. These are protected by file
   system permissions.

   **Best Practices:**
   - Never commit `icetea.toml` to version control
   - Set restrictive permissions: `chmod 600 ~/.config/icetea/icetea.toml`
   - Use separate credentials per environment
   - Rotate credentials regularly
   - Consider using environment variables in CI/CD environments
   ```

### Short-Term Improvements (Robustness)

3. **Add Query Length Limits** (LOW Priority)
   - Prevents accidental paste of huge files
   - Improves stability

4. **Add Query Timeouts** (LOW Priority)
   - Already configured in config.rs (300 seconds default)
   - Ensure it's actually enforced

5. **Add Optional Read-Only Mode** (LOW Priority)
   ```bash
   icetea --read-only  # Disallow DDL/DML operations
   ```

6. **Add Destructive Query Confirmation** (LOW Priority)
   ```rust
   // Prompt for confirmation on DROP, DELETE, TRUNCATE
   if is_destructive(query) {
       confirm("This will modify data. Continue? (y/N): ")?;
   }
   ```

### Long-Term Enhancements (Optional)

7. **Support External Credential Providers**
   - AWS IAM roles
   - Kubernetes service accounts
   - OIDC/OAuth flows

8. **Add Audit Logging**
   - Optional query logging for compliance
   - Useful for shared/bastion hosts

9. **Consider OS Keychain Integration**
   - Optional alternative to plaintext config
   - Use `keyring` crate
   - Still allow plaintext for scriptability

---

## CLI Tool Best Practices (Already Followed)

IceTea already follows many CLI security best practices:

✅ **Uses Configuration Files:** Standard location for user config
✅ **Supports Environment Variables:** `ICETEA_*` prefix
✅ **Provides Example Config:** `icetea.toml.example`
✅ **Graceful Error Handling:** Helpful error messages
✅ **Memory Safe Implementation:** Written in Rust
✅ **No Privilege Escalation:** Runs with user permissions
✅ **Clear Documentation:** Example configs with comments
✅ **Layered Configuration:** Supports multiple config sources

---

## Dependency Health

**Total Dependencies:** 589 crates
**Known CVEs:** 0
**Unmaintained Warnings:** 2 (transitive, low risk)

**Recommendation:** Run `cargo audit` quarterly or before releases

---

## Comparison to Other CLI Tools

| Security Feature | IceTea | AWS CLI | kubectl | psql |
|-----------------|---------|---------|---------|------|
| Plaintext Credentials | ✅ | ✅ | ✅ | ✅ |
| Config in Home Dir | ✅ | ✅ | ✅ | ✅ |
| Environment Variables | ✅ | ✅ | ✅ | ✅ |
| Input Validation | ⚠️ | ⚠️ | ⚠️ | ⚠️ |
| Query Limits | ❌ | ❌ | N/A | ⚠️ |
| Audit Logging | ❌ | ✅ | ✅ | ⚠️ |
| Read-Only Mode | ❌ | ❌ | ❌ | ✅ |

IceTea's security posture is **comparable to industry-standard CLI tools**.

---

## Testing Recommendations

### Security Testing for CLI Tools

1. **Static Analysis:**
   ```bash
   cargo clippy -- -W clippy::all
   cargo audit  # Quarterly
   ```

2. **Robustness Testing:**
   - Test with malformed config files
   - Test with very long queries
   - Test with invalid credentials
   - Test network failure scenarios

3. **Integration Testing:**
   - Test against real Iceberg catalogs
   - Test multi-catalog configurations
   - Test all CLI flags and options

4. **NOT Required for CLI:**
   - ❌ Penetration testing
   - ❌ Web vulnerability scanning
   - ❌ OWASP Top 10 testing
   - ❌ Load testing / DoS testing

---

## Compliance Considerations

### Applicable Standards for CLI Tools

**OWASP Top 10:** Not applicable (web application framework)

**CWE Relevant Items:**
- ✅ CWE-311 (Credential Storage): Acceptable for CLI tools with file permissions
- ✅ CWE-200 (Information Disclosure): Verbose errors expected in CLI tools

**Data Protection Regulations:**
- **GDPR/CCPA:** If processing personal data:
  - ✅ Data minimization (only what user configures)
  - ⚠️ Consider optional audit logging
  - ✅ User controls their own data

- **SOC 2:** If used in enterprise:
  - ⚠️ Consider adding audit logging
  - ✅ Credentials protected by OS
  - ✅ No unnecessary data collection

---

## Conclusion

IceTea follows appropriate security practices for a command-line interface tool. The security model is consistent with industry-standard CLI tools like AWS CLI, kubectl, and psql.

**Security Assessment:**

- ✅ **Memory Safety:** Rust provides strong guarantees
- ✅ **Credential Storage:** Standard CLI pattern with file permissions
- ✅ **Error Handling:** Appropriate verbosity for debugging
- ✅ **Configuration:** Flexible, layered approach
- ⚠️ **Input Validation:** Should add limits for robustness
- ⚠️ **Dependency Monitoring:** Should track updates

**Risk Status:** **LOW - APPROPRIATE FOR CLI TOOL**

**Production Readiness:**
- ✅ Ready for development and internal use now
- ⚠️ Add `.gitignore` entries before team use
- ⚠️ Add query limits before general release
- ✅ Security posture appropriate for 0.1.0 release

**Estimated Effort to Address Recommendations:**
- Critical fixes: **None required**
- High priority (`.gitignore`): **5 minutes**
- Robustness improvements: **1-2 days**
- **Total:** Less than 1 week for all recommendations

---

## Updated Checklist

### Required Before Team Use
- [ ] Add `icetea.toml` to `.gitignore`
- [ ] Document credential storage in README
- [ ] Add example showing file permissions

### Recommended for 1.0 Release
- [ ] Add query length limits (100KB max)
- [ ] Add query timeout enforcement
- [ ] Fix cursor position overflow
- [ ] Add destructive query confirmation
- [ ] Add optional read-only mode
- [ ] Add query logging option

### Nice to Have
- [ ] OS keychain integration
- [ ] OIDC/OAuth support for catalogs
- [ ] Audit logging for enterprise use
- [ ] Shell completion (bash/zsh/fish)

---

## Appendix A: CLI Security Resources

- [The Twelve-Factor CLI Apps](https://medium.com/@jdxcode/12-factor-cli-apps-dd3c227a0e46)
- [CLI Guidelines](https://clig.dev/)
- [AWS CLI Security Best Practices](https://docs.aws.amazon.com/cli/latest/userguide/cli-security-best-practices.html)
- [kubectl Security Context](https://kubernetes.io/docs/tasks/configure-pod-container/security-context/)
- [Rust CLI Book - Configuration](https://rust-cli.github.io/book/tutorial/config.html)

---

## Appendix B: File Permission Recommendations

Recommended permissions for IceTea configuration:

```bash
# Configuration directory
mkdir -p ~/.config/icetea
chmod 700 ~/.config/icetea

# Configuration file
chmod 600 ~/.config/icetea/icetea.toml

# Verify permissions
ls -la ~/.config/icetea
# Should show: drwx------ for directory
# Should show: -rw------- for icetea.toml
```

---

**Report Generated:** 2025-12-31
**Revised:** 2025-12-31 (Updated for CLI threat model)
**Next Review Date:** Before 1.0 release or after significant feature additions

