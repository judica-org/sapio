from typing import TypeVar, List

from sapio.bitcoinlib.script import CScript
from sapio.spending_conditions.clause_to_fragment import FragmentCompiler
from sapio.spending_conditions.flatten_and import FlattenPass
from sapio.spending_conditions.normalize_or import NormalizationPass
from sapio.spending_conditions.opcodes import AllowedOp
from sapio.spending_conditions.script_lang import Clause
from sapio.spending_conditions.witnessmanager import WitnessTemplate, WitnessManager

T = TypeVar('T')

CNF = List[List[Clause]]
class ClauseToCNF:
    def compile_cnf(self, clause: Clause) -> CNF:
        while True:
            normalizer = NormalizationPass()
            clause = normalizer.normalize(clause)
            if not normalizer.took_action:
                break
        return FlattenPass().flatten(clause)


class CNFClauseCompiler:
    def compile(self, cl: List[Clause], w: WitnessTemplate) -> CScript:
        return CScript([FragmentCompiler()._compile(frag, w) for frag in cl])


class ProgramBuilder:

    def compile(self, clause: Clause) -> WitnessManager:
        cnf: CNF = ClauseToCNF().compile_cnf(clause)
        n_cases = len(cnf)
        witness_manager: WitnessManager = WitnessManager()
        # If we have one or two cases, special case the emitted scripts
        # 3 or more, use a generic wrapper
        if n_cases == 1:
            witness = witness_manager.make_witness(0)
            witness_manager.program += CNFClauseCompiler().compile(cnf[0], witness)
            # Hack because the fragment compiler leaves stack empty
            witness_manager.program += CScript([AllowedOp.OP_1])
        elif n_cases == 2:
            wit_0 = witness_manager.make_witness(0)
            wit_1 = witness_manager.make_witness(1)
            wit_0.add(1)
            wit_1.add(0)
            # note order of side effects!
            branch_a = CNFClauseCompiler().compile(cnf[0], wit_0)
            branch_b = CNFClauseCompiler().compile(cnf[1], wit_1)
            witness_manager.program = CScript([AllowedOp.OP_IF,
                                               branch_a,
                                               AllowedOp.OP_ELSE,
                                               branch_b,
                                               AllowedOp.OP_ENDIF,
                                               AllowedOp.OP_1])
        else:
            # If we have more than 3 cases, we can use a nice gadget
            # to emulate a select/jump table in Bitcoin Script.
            # It has an overhead of 5 bytes per branch.
            # Future work can optimize this by inspecting the sub-branches
            # and sharing code...


            # Check that the first argument passed is an in range execution path
            # Note the first branch does not subtract one, so we have arg in [0, N)
            for (idx, cl) in enumerate(cnf):
                wit = witness_manager.make_witness(idx)
                wit.add(idx)
                sub_script = CNFClauseCompiler().compile(cl, wit)
                if idx == 0:
                    witness_manager.program = \
                        CScript([
                            # Verify the top stack item (branch select)
                            # is in range. This is required or else a witness
                            # of e.g. n+1 could steal funds
                            AllowedOp.OP_DUP,
                            AllowedOp.OP_0,
                            n_cases,
                            AllowedOp.OP_WITHIN,
                            AllowedOp.OP_VERIFY,
                            # Successfully range-checked!
                            # If it is 0, do not duplicate as we will take the branch
                            AllowedOp.OP_IFDUP,
                            AllowedOp.OP_NOTIF,
                            sub_script,
                            # We push an OP_0 onto the stack as it will cause
                            # all following branches to not execute,
                            # unless we are the last branch
                            AllowedOp.OP_0,
                            AllowedOp.OP_ENDIF,
                            # set up for testing the next clause...
                            AllowedOp.OP_1SUB])
                elif idx+1 < len(cnf):
                    witness_manager.program += \
                        CScript([AllowedOp.OP_IFDUP,
                                 AllowedOp.OP_NOTIF,
                                 sub_script,
                                 AllowedOp.OP_0,
                                 AllowedOp.OP_ENDIF,
                                 AllowedOp.OP_1SUB])
                # Last clause!
                else:
                    # No ifdup required since we are last, no need for data on
                    # stack
                    # End with an OP_1 so that we succeed after all cases
                    witness_manager.program += \
                        CScript([AllowedOp.OP_NOTIF,
                                 sub_script,
                                 AllowedOp.OP_ENDIF,
                                 AllowedOp.OP_1])

        return witness_manager
