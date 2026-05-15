ORTOOLS_TAG   := v9.15
HIGHS_TAG     := v1.14.0
ORTOOLS_COMMIT := 551ad10d94835c99e5e1e684500d3db398c0e345
HIGHS_COMMIT   := 7df0786de3088c832297e5ed821db236d8fab281
ORTOOLS_SRC   := vendor/ortools
HIGHS_SRC     := vendor/highs
ORTOOLS_BUILD := $(ORTOOLS_SRC)/build
HIGHS_BUILD   := $(HIGHS_SRC)/build
ORTOOLS_VERSION := $(patsubst v%,%,$(ORTOOLS_TAG))
JOBS          := $(shell nproc 2>/dev/null || sysctl -n hw.logicalcpu 2>/dev/null || echo 1)

.PHONY: all ortools highs clean distclean

all: ortools highs

ortools: $(ORTOOLS_BUILD)/.ferrox-$(ORTOOLS_TAG)

$(ORTOOLS_BUILD)/.ferrox-$(ORTOOLS_TAG): Makefile
	@if [ ! -d $(ORTOOLS_SRC) ]; then \
	  git clone --depth 1 --branch $(ORTOOLS_TAG) \
	    https://github.com/google/or-tools $(ORTOOLS_SRC); \
	elif [ ! -d $(ORTOOLS_SRC)/.git ]; then \
	  echo "$(ORTOOLS_SRC) exists but is not a git checkout"; \
	  exit 1; \
	fi
	@set -e; \
	current_tag=$$(git -C $(ORTOOLS_SRC) describe --tags --exact-match 2>/dev/null || true); \
		if [ "$$current_tag" != "$(ORTOOLS_TAG)" ]; then \
		  echo "Switching OR-Tools from $${current_tag:-unknown} to $(ORTOOLS_TAG)"; \
		  git -C $(ORTOOLS_SRC) fetch --depth 1 origin tag $(ORTOOLS_TAG); \
		  git -C $(ORTOOLS_SRC) switch --detach $(ORTOOLS_TAG); \
		  rm -rf $(ORTOOLS_BUILD); \
		fi; \
		actual_commit=$$(git -C $(ORTOOLS_SRC) rev-parse HEAD); \
		if [ "$$actual_commit" != "$(ORTOOLS_COMMIT)" ]; then \
		  echo "OR-Tools $(ORTOOLS_TAG) resolved to $$actual_commit, expected $(ORTOOLS_COMMIT)"; \
		  exit 1; \
		fi
	@if [ -f $(ORTOOLS_BUILD)/ortoolsConfig.cmake ] && \
	    ! grep -q "ORTOOLS_VERSION $(ORTOOLS_VERSION)" $(ORTOOLS_BUILD)/ortoolsConfig.cmake; then \
	  echo "Discarding OR-Tools build that is not $(ORTOOLS_VERSION)"; \
	  rm -rf $(ORTOOLS_BUILD); \
	fi
	cmake -S $(ORTOOLS_SRC) -B $(ORTOOLS_BUILD) \
	  -DCMAKE_BUILD_TYPE=Release \
	  -DBUILD_DEPS=ON \
	  -DBUILD_SHARED_LIBS=ON \
	  -DBUILD_EXAMPLES=OFF \
	  -DBUILD_TESTS=OFF \
	  -DUSE_GLOP=ON \
	  -DUSE_CP_SAT=ON \
	  -DUSE_SCIP=OFF \
	  -DUSE_COINOR=OFF
	cmake --build $(ORTOOLS_BUILD) -j$(JOBS) --target ortools
	@touch $@

highs: $(HIGHS_BUILD)/.ferrox-$(HIGHS_TAG)

$(HIGHS_BUILD)/.ferrox-$(HIGHS_TAG): Makefile
	@if [ ! -d $(HIGHS_SRC) ]; then \
	  git clone --depth 1 --branch $(HIGHS_TAG) \
	    https://github.com/ERGO-Code/HiGHS $(HIGHS_SRC); \
	elif [ ! -d $(HIGHS_SRC)/.git ]; then \
	  echo "$(HIGHS_SRC) exists but is not a git checkout"; \
	  exit 1; \
	fi
	@set -e; \
	current_tag=$$(git -C $(HIGHS_SRC) describe --tags --exact-match 2>/dev/null || true); \
		if [ "$$current_tag" != "$(HIGHS_TAG)" ]; then \
		  echo "Switching HiGHS from $${current_tag:-unknown} to $(HIGHS_TAG)"; \
		  git -C $(HIGHS_SRC) fetch --depth 1 origin tag $(HIGHS_TAG); \
		  git -C $(HIGHS_SRC) switch --detach $(HIGHS_TAG); \
		  rm -rf $(HIGHS_BUILD); \
		fi; \
		actual_commit=$$(git -C $(HIGHS_SRC) rev-parse HEAD); \
		if [ "$$actual_commit" != "$(HIGHS_COMMIT)" ]; then \
		  echo "HiGHS $(HIGHS_TAG) resolved to $$actual_commit, expected $(HIGHS_COMMIT)"; \
		  exit 1; \
		fi
	cmake -S $(HIGHS_SRC) -B $(HIGHS_BUILD) \
	  -DCMAKE_BUILD_TYPE=Release \
	  -DBUILD_SHARED_LIBS=ON \
	  -DFAST_BUILD=ON
	cmake --build $(HIGHS_BUILD) -j$(JOBS)
	@touch $@

clean:
	rm -rf $(ORTOOLS_BUILD) $(HIGHS_BUILD)

distclean:
	rm -rf vendor/
