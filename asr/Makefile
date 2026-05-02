DATA_ROOT = ../corpus/librispeech/flat
RESULT_DIR = results
ALL_F32_TARGETS = test_0_f32 test_1_f32 test_2_f32 test_3_f32 test_4_f32
ALL_M32_TARGETS = test_0_m32 test_1_m32 test_2_m32 test_3_m32 test_4_m32

# Run multiple test directories in a single command
# 1: space-separated test_dirs
# 2: output name prefix
define run_asr_multi
	python3 asr.py --data_root $(DATA_ROOT)/$(2) --preset $(1) \
	 --out out \
	 --gpu_id 0
	mkdir -p $(RESULT_DIR)
	cp out/metrics.json $(RESULT_DIR)/$(1)_$(3)_metrics.json
endef

#  --test_dirs $(1)
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
tsm:
	$(call run_asr_multi,whisper-small,test_small,tsm)

t4f:
	$(call run_asr_multi,whisper-small,test_4_f32,t4f)

t47:
	$(call run_asr_multi,whisper-small,test_4_f377,t4f377)

t4m:
	$(call run_asr_multi,whisper-small,test_4_m32,t4m)

t4: t4f t4m t47

t3f:
	$(call run_asr_multi,whisper-small,test_3_f32,t3f)

t3m:
	$(call run_asr_multi,whisper-small,test_3_m32,t3m)

t37:
	$(call run_asr_multi,whisper-small,test_3_f377,t3f377)

t3: t3f t3m t37

t2f:
	$(call run_asr_multi,whisper-small,test_2_f32,t2f)

t2m:
	$(call run_asr_multi,whisper-small,test_2_m32,t2m)

t27:
	$(call run_asr_multi,whisper-small,test_2_f377,t2f377)

t2: t2f t2m t27

t1f:
	$(call run_asr_multi,whisper-small,test_1_f32,t1f)

t1m:
	$(call run_asr_multi,whisper-small,test_1_m32,t1m)

t17:
	$(call run_asr_multi,whisper-small,test_1_f377,t1f377)

t1: t1f t1m t17

tall: t1 t2 t3

tm1f:
	$(call run_asr_multi,whisper-small,test_-1_f32,tm1f)

tm1m:
	$(call run_asr_multi,whisper-small,test_-1_m32,tm1m)

tm17:
	$(call run_asr_multi,whisper-small,test_-1_f377,tm1f377)

tm1: tm1f tm1m tm17

tm2f:
	$(call run_asr_multi,whisper-small,test_-2_f32,tm2f)

tm2m:
	$(call run_asr_multi,whisper-small,test_-2_m32,tm2m)

tm27:
	$(call run_asr_multi,whisper-small,test_-2_f377,tm2f377)

tm2: tm2f tm2m tm27

tm3f:
	$(call run_asr_multi,whisper-small,test_-3_f32,tm3f)

tm3m:
	$(call run_asr_multi,whisper-small,test_-3_m32,tm3m)

tm37:
	$(call run_asr_multi,whisper-small,test_-3_f377,tm3f377)

tm3: tm3f tm3m tm37

tmall: tm1 tm2 tm3

ttall: tall tmall

tb-small:
	$(call run_asr_multi,whisper-small,enroll,tb-small)

tb-medium:
	$(call run_asr_multi,whisper-medium,enroll,tb-medium)

tb-large:
	$(call run_asr_multi,whisper-large-v3,enroll,tb-large)

tb-wav2vec2:
	$(call run_asr_multi,wav2vec2-960h,enroll,tb-wav2vec2)

tb-xlsr-53:
	$(call run_asr_multi,xlsr-53,enroll,tb-xlsr-53)

tb-all: tb-small tb-medium tb-large tb-wav2vec2 tb-xlsr-53


# Multi-test targets (efficient - single enrollment)
test_all_f32:
	$(call run_asr_multi,$(ALL_F32_TARGETS),f32_tests)

test_all_m32:
	$(call run_asr_multi,$(ALL_M32_TARGETS),m32_tests)

test_all_variants:
	$(call run_asr_multi,$(ALL_F32_TARGETS) $(ALL_M32_TARGETS) enroll,all_variants)

# Default: use efficient multi-test approach
all: test_all_variants

# Clean outputs
clean:
	rm -rf out $(RESULT_DIR)

.PHONY: all ttall tall tmall t4f t4m tb t2f t2m test_all_f32 test_all_m32 test_all_variants clean