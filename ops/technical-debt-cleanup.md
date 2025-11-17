# Technical Debt Cleanup Plan

**Date**: 2025-01-16  
**Status**: Phase 1 Completed ✅  
**Version**: 1.0  
**Last Updated**: 2025-01-16

> **Phase 1 Completion Note**: All dead code has been successfully removed, incorrect `#[allow(dead_code)]` attributes fixed, and all 277 tests are passing. Ready to proceed with Phase 2 (hook migration).

---

## Executive Summary

This document provides a comprehensive analysis of deprecated and legacy code in the Aiki codebase, with a phased removal plan. The analysis found:

- **3 items** safe for immediate removal (dead code)
- **1 critical blocker** requiring migration before removal (production hooks)
- **1 deprecated command** scheduled for removal in v0.2.0
- **2 items** recommended to keep (low maintenance, backward compatibility)

**Total potential cleanup**: ~200 lines of code  
**Risk level**: Low (with proper phased approach)  
**Estimated effort**: 3-4 hours total across 3 phases

---

## Table of Contents

1. [Complete Inventory](#complete-inventory)
2. [Phased Removal Plan](#phased-removal-plan)
3. [Risk Analysis](#risk-analysis)
4. [Migration Guide](#migration-guide)
5. [Testing Impact](#testing-impact)
6. [Documentation Updates](#documentation-updates)

---

## Complete Inventory

### 1. Dead Code (Safe to Remove)

#### 1.1 `JJWorkspace::init_on_existing_git()`

**Location**: `cli/src/jj.rs:34-46`

```rust
#[deprecated(since = "0.1.0", note = "use `init_with_git_dir` instead")]
#[allow(dead_code)]
pub fn init_on_existing_git(&self) -> Result<()> {
    // Implementation...
}
```

**Status**:
- ✅ Marked `#[deprecated]` and `#[allow(dead_code)]`
- ✅ 0 callers found in codebase
- ✅ 0 tests
- ✅ Replaced by `init_with_git_dir()`

**Recommendation**: **Remove immediately** - Pure dead code

**Lines to remove**: ~15

---

#### 1.2 `wait_for_description_update_jjlib()` Test Helper

**Location**: `cli/tests/blame_tests.rs:10`

```rust
#[allow(dead_code)]
fn wait_for_description_update_jjlib(...) -> bool {
    // Implementation...
}
```

**Status**:
- ✅ Unused test helper
- ✅ Alternative implementation exists
- ✅ Not called anywhere

**Recommendation**: **Remove immediately** - Unused test code

**Lines to remove**: ~30

---

#### 1.3 Incorrect `#[allow(dead_code)]` Attributes

**Locations**:
- `cli/src/vendors/cursor.rs:14` - `CursorPayload` struct
- `cli/src/vendors/claude_code.rs:14,29` - `ClaudeCodePayload` and `ToolInput` structs

```rust
#[derive(Deserialize, Debug)]
#[allow(dead_code)]  // ❌ INCORRECT - Used by Serde
struct CursorPayload { ... }
```

**Status**:
- ❌ Misleading attribute
- ✅ Structs ARE actively used (Serde deserialization)
- ✅ Should not be marked dead_code

**Recommendation**: **Remove attribute** - Fix incorrect marking

**Lines to modify**: 3 attributes

---

### 2. Active Deprecated Code (Requires Migration)

#### 2.1 `aiki record-change` CLI Command

**Location**: `cli/src/main.rs:60-67`, `cli/src/commands/record_change.rs`

```rust
/// Record an AI-generated change (called by AI editor hooks)
#[command(name = "record-change")]
RecordChange {
    #[arg(long)]
    claude_code: bool,
    #[arg(long)]
    cursor: bool,
    #[arg(long)]
    sync: bool,
},
```

**Status**:
- ⚠️ Shows deprecation warning when used
- ⚠️ **STILL USED** by `claude-code-plugin/hooks/hooks.json:10`
- ✅ Replaced by `aiki hooks handle --agent <agent> --event <event>`
- ✅ 5 tests in `cli/tests/record_change_tests.rs`

**Current Usage**:
```json
// claude-code-plugin/hooks/hooks.json
"command": "/Users/glasner/code/aiki/cli/target/release/aiki record-change --claude-code"
```

**Recommendation**: **Migrate then remove in v0.2.0**

**Lines to remove**: ~150 (after migration)

---

#### 2.2 `record_change_legacy()` Function

**Location**: `cli/src/record_change.rs:78-106`

```rust
#[allow(dead_code)]
pub fn record_change_legacy(agent_type: AgentType, _sync: bool) -> Result<()> {
    // Implementation...
}
```

**Status**:
- ⚠️ Called by `record-change` command
- ✅ Tests exist
- ✅ Replaced by event bus system

**Recommendation**: **Remove with `record-change` command**

**Lines to remove**: ~30

---

### 3. Legacy Code (Keep for Backward Compatibility)

#### 3.1 Legacy Flow Function Alias

**Location**: `cli/src/flows/executor.rs:418`

```rust
match (module, function) {
    ("core", "build_description") => Self::fn_build_provenance_description(context),
    ("provenance", "build_description") => Self::fn_build_provenance_description(context), // Legacy
    _ => Err(FunctionNotFoundInNamespace(function, module)),
}
```

**Status**:
- ℹ️ Marked `// Legacy`
- ✅ Provides backward compatibility
- ✅ Current flows use new syntax
- ✅ Minimal maintenance burden (2 lines)

**Recommendation**: **Keep indefinitely** - Zero cost backward compatibility

**Rationale**: Breaking user flows for 2 lines of code is not worth it.

---

#### 3.2 `LineAttribution` Struct Fields

**Location**: `cli/src/blame.rs:25,31`

```rust
#[allow(dead_code)]
pub change_id: String,

#[allow(dead_code)]
pub tool_name: Option<String>,
```

**Status**:
- ℹ️ Part of complete data model
- ℹ️ May be needed for future API/features
- ℹ️ Required for complete attribution information

**Recommendation**: **Keep** - Future-proofing

---

#### 3.3 `verify_must_use_triggers_warnings()` Test

**Location**: `cli/src/test_must_use.rs:43`

```rust
#[cfg(any())] // Disabled by default
#[allow(dead_code)]
fn verify_must_use_triggers_warnings() {
    // Examples of what NOT to do
}
```

**Status**:
- ℹ️ Documentation purpose
- ✅ Never compiled (disabled with `#[cfg(any())]`)
- ✅ Shows incorrect usage for learning

**Recommendation**: **Keep** - Serves as documentation

---

## Phased Removal Plan

### Phase 1: Quick Wins (Do Now)

**Timeline**: This week  
**Effort**: 15 minutes  
**Risk**: None  
**Value**: High

**Actions**:
1. ✅ Remove `JJWorkspace::init_on_existing_git()` function
2. ✅ Remove `wait_for_description_update_jjlib()` test helper
3. ✅ Remove `#[allow(dead_code)]` from vendor payload structs

**Files to modify**:
- `cli/src/jj.rs` - Remove function (lines 34-46)
- `cli/tests/blame_tests.rs` - Remove test helper (line 10)
- `cli/src/vendors/cursor.rs` - Remove attribute (line 14)
- `cli/src/vendors/claude_code.rs` - Remove attributes (lines 14, 29)

**Expected outcome**:
- -50 lines of dead code
- More accurate compiler warnings
- No functional changes

**Verification**:
```bash
cargo test  # All tests should still pass
cargo clippy  # Should have fewer warnings
```

---

### Phase 2: Migrate Production Hooks (Critical Path)

**Timeline**: Next sprint (1-2 weeks)  
**Effort**: 2 hours  
**Risk**: Medium (affects production)  
**Value**: Critical (unblocks Phase 3)

**BLOCKER**: `claude-code-plugin/hooks/hooks.json` must be updated first.

**Current state**:
```json
{
  "PostToolUse": {
    "command": "/Users/glasner/code/aiki/cli/target/release/aiki record-change --claude-code"
  }
}
```

**New state**:
```json
{
  "PostToolUse": {
    "command": "/Users/glasner/code/aiki/cli/target/release/aiki hooks handle --agent claude-code --event PostToolUse"
  }
}
```

**Migration steps**:

1. **Update hook configuration**:
   - Modify `claude-code-plugin/hooks/hooks.json`
   - Update command to use new syntax

2. **Test thoroughly**:
   ```bash
   # Test with actual Claude Code usage
   # Verify provenance metadata is recorded
   # Check that Git hooks still work
   # Validate end-to-end workflow
   ```

3. **Update documentation**:
   - `claude-code-plugin/README.md` - Remove `record-change` references
   - Add migration notes for users on old command

4. **Announce deprecation**:
   - 30-day notice before removal
   - Document in CHANGELOG
   - Provide migration guide

**Verification checklist**:
- [ ] Hooks execute successfully
- [ ] Provenance metadata is recorded correctly
- [ ] Git co-author trailers are added
- [ ] No errors in hook execution
- [ ] Documentation is updated

---

### Phase 3: Remove Deprecated Command (Version 0.2.0)

**Timeline**: After Phase 2 is complete and stable  
**Effort**: 30 minutes  
**Risk**: Low (after hook migration)  
**Value**: High (removes ~150 lines)

**Prerequisites**:
- ✅ Phase 2 complete
- ✅ Hooks migrated and tested
- ✅ 30-day deprecation notice given
- ✅ Migration guide published

**Actions**:

1. **Remove command variant**:
   - Delete `Commands::RecordChange` from `cli/src/main.rs`
   - Delete dispatch logic

2. **Remove command module**:
   - Delete `cli/src/commands/record_change.rs`

3. **Remove legacy function**:
   - Remove `record_change_legacy()` from `cli/src/record_change.rs`

4. **Remove tests**:
   - Delete `cli/tests/record_change_tests.rs` (5 tests)

5. **Update documentation** (8 files):
   - `ops/code-review.md`
   - `ops/ROADMAP.md`
   - `ops/phase-1.md`
   - `ops/phase-3.md`
   - `ops/phase-4.md`
   - `ops/CHANGE_ID_IMPLEMENTATION.md`
   - `cli/tests/README_CLAUDE_INTEGRATION.md`
   - `claude-code-plugin/README.md`

6. **Update CHANGELOG**:
   ```markdown
   ## [0.2.0] - YYYY-MM-DD
   
   ### Breaking Changes
   
   - **Removed deprecated `aiki record-change` command**
     - Use `aiki hooks handle --agent <agent> --event <event>` instead
     - Migration guide: [link]
   
   ### Migration Guide
   
   The `record-change` command has been removed. Update your hooks:
   
   **Old syntax**:
   ```
   aiki record-change --claude-code
   ```
   
   **New syntax**:
   ```
   aiki hooks handle --agent claude-code --event PostToolUse
   ```
   ```

**Expected outcome**:
- -150 lines of deprecated code
- Cleaner command structure
- Modern event-based architecture only

**Verification**:
```bash
cargo test  # Ensure all tests pass
cargo build --release  # Verify builds
aiki --help  # Confirm command removed
```

---

## Risk Analysis

### High Risk Items

**Hook migration (Phase 2)**:
- **Risk**: Breaking Claude Code integration
- **Impact**: Users can't record provenance
- **Mitigation**: Thorough testing before rollout
- **Rollback**: Keep old command until migration verified

### Medium Risk Items

**Command removal (Phase 3)**:
- **Risk**: Breaking user scripts
- **Impact**: Users on old command see error
- **Mitigation**: 30-day notice + migration guide
- **Rollback**: Revert commit, restore command

### Low Risk Items

**Dead code removal (Phase 1)**:
- **Risk**: None (code not called)
- **Impact**: None
- **Mitigation**: Not needed
- **Rollback**: Not needed

---

## Migration Guide

### For Users on `record-change` Command

**If you have custom scripts using**:
```bash
aiki record-change --claude-code
```

**Update to**:
```bash
aiki hooks handle --agent claude-code --event PostToolUse
```

**If you have custom hooks.json**:
```json
{
  "PostToolUse": {
    "command": "aiki hooks handle --agent claude-code --event PostToolUse"
  }
}
```

**Why the change?**
- Event-based system is more flexible
- Supports multiple vendors consistently
- Aligns with Aiki's architecture
- Enables future WASM extensions

**Need help?**
- Check documentation: `docs/hooks.md`
- Run `aiki hooks --help`
- File issue: github.com/user/aiki/issues

---

## Testing Impact

### Tests to Remove

**With `record-change` command** (Phase 3):
```
cli/tests/record_change_tests.rs
├── test_record_change_requires_agent_flag
├── test_record_change_fails_with_invalid_json
├── test_record_change_handles_write_tool
├── test_record_change_with_valid_json
└── test_record_change_with_sync_flag
```

**Total**: 5 tests

### Replacement Coverage

Ensure equivalent coverage exists in:
```
cli/tests/git_hooks_tests.rs
├── test_hook_runs_for_normal_commits
├── test_git_hook_includes_multiple_editors
└── ... (15 tests total)
```

**Action**: Verify `git_hooks_tests.rs` covers same scenarios.

### Test Count Impact

- **Before Phase 1**: 277 tests
- **After Phase 1**: 277 tests (no change)
- **After Phase 3**: 272 tests (-5 from removing record_change_tests.rs)

**Replacement**: Hook tests in `git_hooks_tests.rs` provide coverage.

---

## Documentation Updates

### Files Requiring Updates (Phase 3)

1. **`ops/code-review.md`**:
   - Remove deprecation action item
   - Update to reference new command

2. **`ops/ROADMAP.md`**:
   - Update Phase 3 section
   - Remove `record-change` references

3. **`ops/phase-1.md`**:
   - Update implementation documentation
   - Show new command syntax

4. **`ops/phase-3.md`**:
   - Update hook command examples
   - Document new event-based system

5. **`ops/phase-4.md`**:
   - Update signing workflow examples
   - Use new command syntax

6. **`ops/CHANGE_ID_IMPLEMENTATION.md`**:
   - Update testing notes
   - Remove old command references

7. **`cli/tests/README_CLAUDE_INTEGRATION.md`**:
   - Update test documentation
   - Show new integration approach

8. **`claude-code-plugin/README.md`** ⚠️ **USER-FACING**:
   - Update installation instructions
   - Document migration from old command
   - Provide examples with new syntax

### New Documentation to Create

1. **`docs/migration-0.2.0.md`**:
   - Complete migration guide
   - Before/after examples
   - Troubleshooting section

2. **`CHANGELOG.md` section**:
   - Breaking changes
   - Migration guide link
   - Deprecation timeline

---

## Timeline & Milestones

### Week 1 (Phase 1) ✅ COMPLETED
- [x] Remove dead code
- [x] Fix incorrect attributes
- [x] Verify all tests pass
- [ ] Create PR with cleanup (if needed)

### Week 2-3 (Phase 2)
- [ ] Update hooks.json
- [ ] Test with real Claude Code usage
- [ ] Update plugin documentation
- [ ] Deploy to beta testers

### Week 4
- [ ] Announce deprecation timeline
- [ ] Publish migration guide
- [ ] Monitor for issues

### Version 0.2.0 (Phase 3)
- [ ] Remove deprecated command
- [ ] Update all documentation
- [ ] Add to CHANGELOG
- [ ] Release with migration notes

---

## Success Criteria

### Phase 1
- ✅ All tests pass (277/277)
- ✅ No compilation warnings for removed code
- ✅ Zero functional changes

### Phase 2
- ✅ Hooks execute without errors
- ✅ Provenance metadata recorded correctly
- ✅ No user-reported issues for 2 weeks

### Phase 3
- ✅ Command successfully removed
- ✅ Tests pass (272/272)
- ✅ Documentation updated
- ✅ Migration guide available
- ✅ Zero regression reports

---

## Rollback Plan

### If Phase 2 Fails
```bash
# Revert hooks.json
git checkout HEAD -- claude-code-plugin/hooks/hooks.json

# Keep old command active
# Investigate issues
# Fix and retry
```

### If Phase 3 Causes Issues
```bash
# Revert the removal commit
git revert <commit-hash>

# Restore deprecated command temporarily
# Document issues
# Plan better migration
```

---

## Recommendations

### Do Now (Phase 1)
✅ **Remove dead code** - No risk, immediate value

### Do Next Sprint (Phase 2)
⚠️ **Migrate hooks carefully** - Critical but manageable

### Do in 0.2.0 (Phase 3)
🗑️ **Remove deprecated command** - Proper breaking change

### Consider Keeping Forever
🤷 **Legacy function alias** - Zero maintenance cost

### Document
📝 **Add deprecation policy to CLAUDE.md** - Prevent future confusion

---

## Appendix: Code Inventory Details

### Files Modified (Phase 1)
```
cli/src/jj.rs                           -13 lines
cli/tests/blame_tests.rs                -28 lines
cli/src/vendors/cursor.rs               -1 line (attribute)
cli/src/vendors/claude_code.rs          -2 lines (attributes)
```

### Files Deleted (Phase 3)
```
cli/src/commands/record_change.rs       -24 lines
cli/tests/record_change_tests.rs        -127 lines
```

### Total Cleanup
- **Phase 1**: ~45 lines
- **Phase 3**: ~151 lines
- **Total**: ~196 lines removed

---

## Questions & Answers

**Q: Why not remove everything now?**  
A: Production hooks still use `record-change`. Migration required first.

**Q: Why keep the legacy function alias?**  
A: Only 2 lines of code, provides backward compatibility, zero maintenance.

**Q: When exactly is 0.2.0 release?**  
A: After Phase 2 is complete and stable (estimated 4-6 weeks).

**Q: What if users complain about removal?**  
A: We have a 30-day notice period and complete migration guide.

**Q: Are there alternatives to removal?**  
A: No - deprecated code should be removed eventually. This is normal lifecycle.

---

## Conclusion

This technical debt cleanup follows a responsible phased approach:

1. **Phase 1**: Safe, immediate wins (dead code removal)
2. **Phase 2**: Careful migration (production hooks)
3. **Phase 3**: Proper deprecation (0.2.0 breaking change)

The plan balances:
- ✅ Code quality improvement
- ✅ Backward compatibility concerns
- ✅ User migration support
- ✅ Risk mitigation

**Total effort**: 3-4 hours over 4-6 weeks  
**Total value**: Cleaner codebase, modern architecture  
**Risk level**: Low (with phased approach)

**Status**: Ready for implementation - Phase 1 can start immediately.
