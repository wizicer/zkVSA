DATA_ROOT = ../../corpus/librispeech/flat
RESULT_DIR = results
ALL_F32_TARGETS = test_-3_f32 test_-2_f32 test_-1_f32 test_1_f32 test_2_f32 test_3_f32
ALL_M32_TARGETS = test_-3_m32 test_-2_m32 test_-1_m32 test_1_m32 test_2_m32 test_3_m32
ALL_F377_TARGETS = test_-3_f377 test_-2_f377 test_-1_f377 test_1_f377 test_2_f377 test_3_f377


# Run multiple test directories in a single command
# 1: space-separated test_dirs
# 2: output name prefix
define run_asv_multi
	python3 asv.py --data_root $(DATA_ROOT) --out out --test_dirs $(1) --gpu_id 0
	mkdir -p $(RESULT_DIR)
	cp out/summary.json $(RESULT_DIR)/$(2)_summary.json
endef

#  --save_plots

# @for dir in $(1); do \
# 	if [ -f out/$$dir/metrics.json ]; then \
# 		cp out/$$dir/metrics.json $(RESULT_DIR)/$(2)_$$dir.json; \
# 	fi; \
# 	if [ -f out/$$dir/det.png ]; then \
# 		cp out/$$dir/det.png $(RESULT_DIR)/$(2)_$$dir_det.png; \
# 	fi; \
# 	if [ -f out/$$dir/scores.csv ]; then \
# 		cp out/$$dir/scores.csv $(RESULT_DIR)/$(2)_$$dir_scores.csv; \
# 	fi; \
# done

.DEFAULT_GOAL := all

# Individual test targets (backward compatibility)
t4f:
	$(call run_asv_multi,test_4_f32,t4f)

t4m:
	$(call run_asv_multi,test_4_m32,t4m)

tb:
	$(call run_asv_multi,enroll,tb)

t2f:
	$(call run_asv_multi,test_2_f32,t2f)

t2m:
	$(call run_asv_multi,test_2_m32,t2m)

# Multi-test targets (efficient - single enrollment)
test_all_f32:
	$(call run_asv_multi,$(ALL_F32_TARGETS),f32_tests)

test_all_m32:
	$(call run_asv_multi,$(ALL_M32_TARGETS),m32_tests)

test_all_f377:
	$(call run_asv_multi,$(ALL_F377_TARGETS),f377_tests)

test_all_variants:
	$(call run_asv_multi,$(ALL_F32_TARGETS) $(ALL_M32_TARGETS) $(ALL_F377_TARGETS) enroll,all_variants)

# Default: use efficient multi-test approach
all: test_all_variants

# Clean outputs
clean:
	rm -rf out $(RESULT_DIR)

.PHONY: all t4f t4m tb t2f t2m test_all_f32 test_all_m32 test_all_f377 test_all_variants clean